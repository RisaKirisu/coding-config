mod common;

use axum::Router;
use common::{echo_headers_handler, CaddyGuard};
use reqwest::StatusCode;
use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_caddy_loopback_facade_and_routing() {
    // 1. Start mock upstream echo server on an ephemeral port
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();
    let upstream_port = upstream_addr.port();

    let app = Router::new().fallback(echo_headers_handler);

    tokio::spawn(async move {
        axum::serve(upstream_listener, app).await.unwrap();
    });

    // 2. Find an ephemeral port for Caddy
    let caddy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let caddy_port = caddy_listener.local_addr().unwrap().port();
    drop(caddy_listener);

    // 3. Prepare Caddyfile from scripts/Caddyfile with test port
    let template_caddyfile =
        fs::read_to_string("scripts/Caddyfile").expect("scripts/Caddyfile must exist");

    // Replace default port :10080 with :caddy_port
    let test_caddyfile_content = template_caddyfile.replace(":10080", &format!(":{}", caddy_port));

    let temp_dir = tempdir().unwrap();
    let caddyfile_path = temp_dir.path().join("Caddyfile");
    fs::write(&caddyfile_path, test_caddyfile_content).unwrap();

    // 4. Start real Caddy process
    let child = Command::new("caddy")
        .arg("run")
        .arg("--config")
        .arg(&caddyfile_path)
        .arg("--adapter")
        .arg("caddyfile")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start Caddy");

    let _guard = CaddyGuard(Some(child));

    // Wait for Caddy to start listening
    let client = reqwest::Client::builder().build().unwrap();
    let caddy_url = format!("http://127.0.0.1:{}/test-path", caddy_port);

    let mut started = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(res) = client
            .get(&caddy_url)
            .header(
                "Host",
                format!("{}.test-proj.devvm.localhost", upstream_port),
            )
            .send()
            .await
        {
            if res.status().is_success() {
                started = true;
                break;
            }
        }
    }
    assert!(started, "Caddy failed to start within timeout");

    // Case 1: Request with .devvm.localhost Host and Origin
    let host_header = format!(
        "{}.my-proj-12345678.devvm.localhost:{}",
        upstream_port, caddy_port
    );
    let origin_header = format!(
        "http://{}.my-proj-12345678.devvm.localhost:{}",
        upstream_port, caddy_port
    );

    let res = client
        .get(&caddy_url)
        .header("Host", &host_header)
        .header("Origin", &origin_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let headers: serde_json::Value = res.json().await.unwrap();

    // Loopback Facade Invariant: Application sees loopback Host and Origin
    assert_eq!(headers["host"], format!("localhost:{}", upstream_port));
    assert_eq!(
        headers["origin"],
        format!("http://localhost:{}", upstream_port)
    );

    // Case 2: Request with .devvm.internal Host and Origin (Tailnet Project URL)
    let tailnet_host = format!("{}.my-proj-12345678.devvm.internal", upstream_port);
    let tailnet_origin = format!("http://{}.my-proj-12345678.devvm.internal", upstream_port);

    let res = client
        .get(&caddy_url)
        .header("Host", &tailnet_host)
        .header("Origin", &tailnet_origin)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let headers: serde_json::Value = res.json().await.unwrap();

    assert_eq!(headers["host"], format!("localhost:{}", upstream_port));
    assert_eq!(
        headers["origin"],
        format!("http://localhost:{}", upstream_port)
    );

    // Case 3: Request without Origin header (plain GET) -> Upstream receives Host, no Origin injected
    let res = client
        .get(&caddy_url)
        .header("Host", &host_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let headers: serde_json::Value = res.json().await.unwrap();
    assert_eq!(headers["host"], format!("localhost:{}", upstream_port));
    assert!(headers.get("origin").is_none());

    // Case 4: Non-matching host -> 400 Bad Request
    let res = client
        .get(&caddy_url)
        .header("Host", "attacker.com")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let text = res.text().await.unwrap();
    assert!(text.contains("Invalid devvm hostname"));

    // Case 5: Genuinely cross-origin Origin -> Origin header preserved as-is, not rewritten to localhost
    let cross_origin = "https://evil.com";
    let res = client
        .get(&caddy_url)
        .header("Host", &host_header)
        .header("Origin", cross_origin)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let headers: serde_json::Value = res.json().await.unwrap();
    assert_eq!(headers["host"], format!("localhost:{}", upstream_port));
    assert_eq!(headers["origin"], "https://evil.com");
}
