mod common;

use common::{echo_headers_handler, CaddyGuard};
use devvm_daemon::{create_router, AppState, DaemonConfig, DshRuntimeManager, SyncManager};
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_remote_http_proxy_chain_and_dsh_auth() {
    let temp_dir = tempdir().unwrap();

    // 1. Upstream application servers:
    // Port app1_port: Project 1 application (echo headers)
    let app1_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app1_port = app1_listener.local_addr().unwrap().port();
    let app1 = axum::Router::new().fallback(echo_headers_handler);
    tokio::spawn(async move {
        axum::serve(app1_listener, app1).await.unwrap();
    });

    // Port app2_port: Project 2 application (echo headers)
    let app2_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app2_port = app2_listener.local_addr().unwrap().port();
    let app2 = axum::Router::new().fallback(echo_headers_handler);
    tokio::spawn(async move {
        axum::serve(app2_listener, app2).await.unwrap();
    });

    // 2. Start Guest Caddy (reads scripts/Caddyfile, test port replaces 10080)
    let guest_caddy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let guest_caddy_port = guest_caddy_listener.local_addr().unwrap().port();
    drop(guest_caddy_listener);

    let template_caddyfile =
        fs::read_to_string("scripts/Caddyfile").expect("scripts/Caddyfile must exist");
    let guest_caddy_content =
        template_caddyfile.replace(":10080", &format!(":{}", guest_caddy_port));
    let guest_caddyfile_path = temp_dir.path().join("GuestCaddyfile");
    fs::write(&guest_caddyfile_path, guest_caddy_content).unwrap();

    let guest_caddy_child = Command::new("caddy")
        .arg("run")
        .arg("--config")
        .arg(&guest_caddyfile_path)
        .arg("--adapter")
        .arg("caddyfile")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start Guest Caddy");
    let _guest_guard = CaddyGuard(Some(guest_caddy_child));

    // 3. Start FRP components (frps + frpc)
    let frps_bin = dirs::home_dir()
        .map(|h| h.join(".local/bin/frps"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("frps"));

    let frps_bind_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let frps_bind_port = frps_bind_listener.local_addr().unwrap().port();
    drop(frps_bind_listener);

    let frps_vhost_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let frps_vhost_port = frps_vhost_listener.local_addr().unwrap().port();
    drop(frps_vhost_listener);

    let frps_child = Command::new(&frps_bin)
        .args([
            "--bind-addr",
            "127.0.0.1",
            "--bind-port",
            &frps_bind_port.to_string(),
            "--proxy-bind-addr",
            "127.0.0.1",
            "--vhost-http-port",
            &frps_vhost_port.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start frps");
    let _frps_guard = CaddyGuard(Some(frps_child));

    // Wait for frps to be listening on bind port
    let mut frps_ready = false;
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(("127.0.0.1", frps_bind_port))
            .await
            .is_ok()
        {
            frps_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(frps_ready, "frps failed to bind within timeout");

    let frpc_toml_content = format!(
        r#"serverAddr = "127.0.0.1"
serverPort = {frps_bind_port}

[[proxies]]
name = "proj-alpha"
type = "http"
localIP = "127.0.0.1"
localPort = {guest_caddy_port}
customDomains = [
	"*.proj-alpha-11111111.devvm.localhost",
]

[[proxies]]
name = "proj-beta"
type = "http"
localIP = "127.0.0.1"
localPort = {guest_caddy_port}
customDomains = [
	"*.proj-beta-22222222.devvm.localhost",
]

[[proxies]]
name = "proj-dsh"
type = "http"
localIP = "127.0.0.1"
localPort = {guest_caddy_port}
customDomains = [
	"*.proj-dsh-33333333.devvm.localhost",
]
"#
    );
    let frpc_toml_path = temp_dir.path().join("frpc.toml");
    fs::write(&frpc_toml_path, frpc_toml_content).unwrap();

    let frpc_child = Command::new("frpc")
        .arg("-c")
        .arg(&frpc_toml_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start frpc");
    let _frpc_guard = CaddyGuard(Some(frpc_child));

    // 4. Start Real Control Daemon using create_router
    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path: config_dir.join("projects.json"),
        sync_config_path: config_dir.join("sync.json"),
        log_dir: temp_dir.path().join("logs"),
        home_dir: temp_dir.path().join("home"),
        devvm_bin: PathBuf::from("true"),
        ingress_port: frps_vhost_port,
        remote_domain: Some("risak.dev".to_string()),
    };
    let app_state = AppState {
        config: config.clone(),
        dsh_runtime_manager: DshRuntimeManager::new(),
        sync_manager: SyncManager::new(),
    };
    let control_router = create_router(app_state);
    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_port = control_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(control_listener, control_router).await.unwrap();
    });

    // 5. Start Host Caddy routing through FRP (frps_vhost_port)
    let host_caddy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host_caddy_port = host_caddy_listener.local_addr().unwrap().port();
    drop(host_caddy_listener);

    // Exercise the shipped routes, replacing only TLS and addresses for this offline HTTP test.
    let host_caddy_content = format!(
        "{{\n admin off\n auto_https off\n}}\n{}",
        include_str!("../scripts/Caddyfile.host")
    )
    .replace(
        "https://devvm.{$REMOTE_DOMAIN:risak.dev}, https://*.{$REMOTE_DOMAIN:risak.dev}",
        &format!(":{host_caddy_port}"),
    )
    .replace("{$REMOTE_DOMAIN:risak.dev}", "risak.dev")
    .replace("{$REMOTE_DOMAIN_REGEXP:risak\\.dev}", "risak\\.dev")
    .replace("{$REMOTE_IP:100.67.154.69}", "127.0.0.1")
    .replace(
        "\ttls {\n\t\tdns cloudflare {env.CLOUDFLARE_API_TOKEN}\n\t}\n",
        "",
    )
    .replace("127.0.0.1:8100", &format!("127.0.0.1:{control_port}"))
    .replace(":8102", &format!(":{frps_vhost_port}"));
    let host_caddyfile_path = temp_dir.path().join("HostCaddyfile");
    fs::write(&host_caddyfile_path, host_caddy_content).unwrap();

    let host_caddy_child = Command::new("caddy")
        .arg("run")
        .arg("--config")
        .arg(&host_caddyfile_path)
        .arg("--adapter")
        .arg("caddyfile")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start Host Caddy");
    let _host_guard = CaddyGuard(Some(host_caddy_child));

    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap();

    // Wait for Host Caddy and FRP chain to be ready
    let mut ready = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(res) = client
            .get(format!("http://127.0.0.1:{}/api/projects", host_caddy_port))
            .header("Host", "devvm.risak.dev")
            .send()
            .await
        {
            if res.status().is_success() {
                ready = true;
                break;
            }
        }
    }
    assert!(
        ready,
        "Host Caddy and FRP chain failed to start within timeout"
    );

    let host_base = format!("http://127.0.0.1:{}", host_caddy_port);

    // --- TEST 1: Host & Origin Translation across Host Caddy -> FRP -> Guest Caddy ---
    let proj1_host = format!("proj-alpha-11111111-{}.risak.dev", app1_port);
    let proj1_origin = format!("https://{}", proj1_host);
    let mut routed_p1 = false;
    let mut headers1: Value = Value::Null;
    for _ in 0..30 {
        let res = client
            .get(format!("{}/echo", host_base))
            .header("Host", &proj1_host)
            .header("Origin", &proj1_origin)
            .send()
            .await;
        if let Ok(res1) = res {
            let status = res1.status();
            if status == StatusCode::OK {
                headers1 = res1.json().await.unwrap();
                routed_p1 = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(routed_p1, "Project 1 failed to route through FRP chain");
    assert_eq!(headers1["host"], format!("localhost:{}", app1_port));
    assert_eq!(
        headers1["origin"],
        format!("http://localhost:{}", app1_port)
    );

    // --- TEST 2: Multiple Projects and Ports (Project 2 on app2_port) ---
    let proj2_host = format!("proj-beta-22222222-{}.risak.dev", app2_port);
    let proj2_origin = format!("https://{}", proj2_host);
    let res2 = client
        .get(format!("{}/echo", host_base))
        .header("Host", &proj2_host)
        .header("Origin", &proj2_origin)
        .send()
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let headers2: Value = res2.json().await.unwrap();
    assert_eq!(headers2["host"], format!("localhost:{}", app2_port));
    assert_eq!(
        headers2["origin"],
        format!("http://localhost:{}", app2_port)
    );

    // --- TEST 3: Unrelated Origin Header Preserved (Security: not rewritten) ---
    let res3 = client
        .get(format!("{}/echo", host_base))
        .header("Host", &proj1_host)
        .header("Origin", "https://unrelated-attacker.com")
        .send()
        .await
        .unwrap();
    assert_eq!(res3.status(), StatusCode::OK);
    let headers3: Value = res3.json().await.unwrap();
    assert_eq!(headers3["host"], format!("localhost:{}", app1_port));
    assert_eq!(headers3["origin"], "https://unrelated-attacker.com");

    // --- TEST 4: Control Daemon Routing (reaches real create_router) ---
    let res_ctrl = client
        .get(format!("{}/api/projects", host_base))
        .header("Host", "devvm.risak.dev")
        .send()
        .await
        .unwrap();
    assert_eq!(res_ctrl.status(), StatusCode::OK);
    let ctrl_data: Value = res_ctrl.json().await.unwrap();
    assert!(ctrl_data.is_array());

    // --- TEST 5: Rejection of Unknown Host Shapes ---
    let res_bad1 = client
        .get(format!("{}/echo", host_base))
        .header("Host", "unrelated-wildcard.risak.dev")
        .send()
        .await
        .unwrap();
    assert_eq!(res_bad1.status(), StatusCode::BAD_REQUEST);

    let res_bad2 = client
        .get(format!("{}/echo", host_base))
        .header("Host", "arbitrary-attacker.com")
        .send()
        .await
        .unwrap();
    assert_eq!(res_bad2.status(), StatusCode::BAD_REQUEST);

    // --- TEST 6: Real Installed DSH Runtime Browser Auth & Streaming ---
    let real_token = fs::read_to_string("/tmp/devvm-daemon-dsh.token")
        .expect("Installed DSH launch token must be present at /tmp/devvm-daemon-dsh.token");
    let trimmed_token = real_token.trim();
    assert!(
        !trimmed_token.is_empty(),
        "DSH launch token must not be empty"
    );

    let dsh_remote_host = "proj-dsh-33333333-3080.risak.dev";

    // 6a. Direct access without token or cookie -> Real DSH returns 401 Unauthorized
    let unauth_res = client
        .get(format!("{}/", host_base))
        .header("Host", dsh_remote_host)
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);
    let unauth_body = unauth_res.text().await.unwrap();
    assert!(unauth_body.contains("dsh web authentication required"));

    // 6b. Navigation with real launch token -> 303 See Other with SameSite=Strict cookie
    let token_res = client
        .get(format!("{}/?token={}", host_base, trimmed_token))
        .header("Host", dsh_remote_host)
        .send()
        .await
        .unwrap();
    assert_eq!(token_res.status(), StatusCode::SEE_OTHER);
    assert_eq!(token_res.headers().get("location").unwrap(), "/");

    let set_cookie = token_res
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("SameSite=Strict"));

    let cookie_pair = set_cookie.split(';').next().unwrap();

    // 6c. Replay the cookie to verify server authentication. This is not a browser SameSite test.
    let mut redirected_res = client
        .get(format!("{}/", host_base))
        .header("Host", dsh_remote_host)
        .header("Cookie", cookie_pair)
        .send()
        .await
        .unwrap();
    assert_eq!(redirected_res.status(), StatusCode::OK);
    let transfer_encoding = redirected_res
        .headers()
        .get("transfer-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        transfer_encoding.contains("chunked"),
        "DSH runtime response must be chunked streaming, got headers: {:?}",
        redirected_res.headers()
    );

    let mut stream_bytes = Vec::new();
    while let Some(chunk) = redirected_res.chunk().await.unwrap() {
        stream_bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&stream_bytes);
    assert!(body.contains("<!doctype html>"));

    // --- TEST 7: WebSocket Upgrade against Installed DSH Runtime through the Proxy Chain ---
    let mut tcp_stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", host_caddy_port))
        .await
        .expect("Failed to connect TCP to Host Caddy");

    let ws_request = format!(
        "GET /sidebar/ws/agent-terminals HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Cookie: {}\r\n\
         \r\n",
        dsh_remote_host, cookie_pair
    );
    tcp_stream.write_all(ws_request.as_bytes()).await.unwrap();

    let mut response_buf = [0u8; 1024];
    let n = tcp_stream.read(&mut response_buf).await.unwrap();
    let response_str = String::from_utf8_lossy(&response_buf[..n]);

    assert!(
        response_str.starts_with("HTTP/1.1 101 Switching Protocols")
            || response_str.starts_with("HTTP/1.1 101"),
        "Expected 101 Switching Protocols from live DSH runtime for WebSocket upgrade, got:\n{}",
        response_str
    );
    assert!(response_str.to_lowercase().contains("upgrade: websocket"));
    assert!(response_str.to_lowercase().contains("connection: upgrade"));
}
