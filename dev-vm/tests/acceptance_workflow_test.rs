mod common;

use axum::Router;
use common::{
    build_dns_query, create_mock_devvm, echo_headers_handler, parse_dns_response, CaddyGuard,
    FakeSyncRunner,
};
use devvm_daemon::{
    create_router, AppState, DaemonConfig, DnsConfig, DnsServer, DshRuntimeManager, SyncManager,
};
use reqwest::StatusCode as ReqwestStatusCode;
use serde_json::{json, Value};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::watch;
use uuid::Uuid;

struct AcceptanceContext {
    _temp_dir: tempfile::TempDir,
    home_dir: PathBuf,
    server_addr: SocketAddr,
    caddy_port: u16,
    dns_addr: SocketAddr,
    echo_port: u16,
    client: reqwest::Client,
    fake_runner: Arc<FakeSyncRunner>,
    _dns_shutdown_tx: watch::Sender<bool>,
    _caddy_guard: CaddyGuard,
}

struct RegisteredProject {
    id: Uuid,
    id_str: String,
    host: String,
    dir: PathBuf,
}

async fn setup_acceptance_system() -> AcceptanceContext {
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("projects.json");
    let sync_config_path = config_dir.join("sync.json");

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let devvm_bin = temp_dir.path().join("mock_devvm");
    create_mock_devvm(&devvm_bin);

    // 1. Start Mock Upstream Echo Server (handles proxied traffic)
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo_listener.local_addr().unwrap();
    let echo_port = echo_addr.port();

    let echo_app = Router::new().fallback(echo_headers_handler);
    tokio::spawn(async move {
        axum::serve(echo_listener, echo_app).await.unwrap();
    });

    // 2. Start Real Caddy with test port
    let caddy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let caddy_port = caddy_listener.local_addr().unwrap().port();
    drop(caddy_listener);

    let template_caddyfile =
        fs::read_to_string("scripts/Caddyfile").expect("scripts/Caddyfile must exist");
    let test_caddyfile_content = template_caddyfile.replace(":10080", &format!(":{}", caddy_port));
    let caddyfile_path = temp_dir.path().join("Caddyfile");
    fs::write(&caddyfile_path, test_caddyfile_content).unwrap();

    let caddy_child = Command::new("caddy")
        .arg("run")
        .arg("--config")
        .arg(&caddyfile_path)
        .arg("--adapter")
        .arg("caddyfile")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start Caddy");

    let caddy_guard = CaddyGuard(Some(caddy_child));

    // 3. Start Real Wildcard DNS Server
    let dns_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_addr = dns_socket.local_addr().unwrap();

    let dns_config = DnsConfig {
        bind_addr: dns_addr.to_string(),
        target_ip: Ipv4Addr::new(100, 64, 0, 42),
        domain: "devvm.internal".to_string(),
        target_ipv6: None,
        ttl: 60,
    };

    let (dns_shutdown_tx, dns_shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        DnsServer::run_with_socket(dns_socket, dns_config, Some(dns_shutdown_rx))
            .await
            .unwrap();
    });

    // 4. Start Control Daemon HTTP API
    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path,
        sync_config_path,
        log_dir,
        home_dir: home_dir.clone(),
        devvm_bin,
        ingress_port: caddy_port,
        tailnet_domain: "devvm.internal".to_string(),
    };

    let fake_runner = Arc::new(FakeSyncRunner::new());
    let sync_manager = SyncManager::with_runner(fake_runner.clone());
    let dsh_runtime_manager = DshRuntimeManager::new();

    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager,
        sync_manager,
    };

    let router = create_router(state);
    let daemon_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = daemon_listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(daemon_listener, router).await.unwrap();
    });

    let client = reqwest::Client::builder().build().unwrap();

    // Wait for Caddy to be ready
    let test_url = format!("http://127.0.0.1:{}/ready-check", caddy_port);
    let mut caddy_ready = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(res) = client
            .get(&test_url)
            .header(
                "Host",
                format!("{}.ready.devvm.localhost:{}", echo_port, caddy_port),
            )
            .send()
            .await
        {
            if res.status().is_success() {
                caddy_ready = true;
                break;
            }
        }
    }
    assert!(caddy_ready, "Caddy failed to become ready within timeout");

    AcceptanceContext {
        _temp_dir: temp_dir,
        home_dir,
        server_addr,
        caddy_port,
        dns_addr,
        echo_port,
        client,
        fake_runner,
        _dns_shutdown_tx: dns_shutdown_tx,
        _caddy_guard: caddy_guard,
    }
}

/// Sub-step helper for Step 1: Project Browser Jail Verification
async fn assert_step_1_browser_jail(ctx: &AcceptanceContext, workspace_dir: &Path) {
    let outside_dir = ctx._temp_dir.path().join("forbidden_outside");
    fs::create_dir_all(&outside_dir).unwrap();

    // Default root lists home directory
    let browser_res = ctx
        .client
        .get(format!("http://{}/api/browser", ctx.server_addr))
        .send()
        .await
        .unwrap();
    assert_eq!(browser_res.status(), ReqwestStatusCode::OK);
    let browser_json: Value = browser_res.json().await.unwrap();
    assert_eq!(
        browser_json["current"].as_str().unwrap(),
        fs::canonicalize(&ctx.home_dir).unwrap().to_str().unwrap()
    );
    let entries = browser_json["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["name"] == "workspace"));

    // Subdirectory navigation within home
    let sub_res = ctx
        .client
        .get(format!(
            "http://{}/api/browser?path={}",
            ctx.server_addr,
            workspace_dir.to_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(sub_res.status(), ReqwestStatusCode::OK);

    // Browsing outside home jail is forbidden (403)
    let jail_res1 = ctx
        .client
        .get(format!(
            "http://{}/api/browser?path={}",
            ctx.server_addr,
            outside_dir.to_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(jail_res1.status(), ReqwestStatusCode::FORBIDDEN);

    // Directory traversal escaping home jail is forbidden (403)
    let jail_res2 = ctx
        .client
        .get(format!(
            "http://{}/api/browser?path={}/../../",
            ctx.server_addr,
            ctx.home_dir.to_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(jail_res2.status(), ReqwestStatusCode::FORBIDDEN);
}

/// Sub-step helper for Step 2: Project Registration & UUID .devvm-id Lifecycle
async fn assert_step_2_project_registration_and_id_lifecycle(
    ctx: &AcceptanceContext,
    workspace_dir: &Path,
) -> RegisteredProject {
    let project_a_dir = workspace_dir.join("project-alpha");
    fs::create_dir_all(&project_a_dir).unwrap();

    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": project_a_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg_res.status(), ReqwestStatusCode::OK);
    let proj_a: Value = reg_res.json().await.unwrap();
    let project_a_id_str = proj_a["id"].as_str().unwrap().to_string();
    let project_a_id = Uuid::parse_str(&project_a_id_str).expect("Must return valid UUID");
    let project_a_host = proj_a["project_host"].as_str().unwrap().to_string();

    assert_eq!(proj_a["name"], "project-alpha");
    assert_eq!(proj_a["vm_status"], "stopped");
    assert_eq!(proj_a["dsh_status"], "stopped");
    assert_eq!(proj_a["sync_status"], "not_configured");

    // Verify .devvm-id file was created with exact UUID
    let id_file = project_a_dir.join(".devvm-id");
    assert!(
        id_file.exists(),
        ".devvm-id file must be created on registration"
    );
    let stored_id = fs::read_to_string(&id_file).unwrap();
    assert_eq!(stored_id.trim(), project_a_id_str.as_str());

    // Re-registering reuses existing .devvm-id
    let rereg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": project_a_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(rereg_res.status(), ReqwestStatusCode::OK);
    let proj_a_rereg: Value = rereg_res.json().await.unwrap();
    assert_eq!(proj_a_rereg["id"], project_a_id_str.as_str());

    // List projects endpoint
    let list_res = ctx
        .client
        .get(format!("http://{}/api/projects", ctx.server_addr))
        .send()
        .await
        .unwrap();
    assert_eq!(list_res.status(), ReqwestStatusCode::OK);
    let list_json: Vec<Value> = list_res.json().await.unwrap();
    assert_eq!(list_json.len(), 1);
    assert_eq!(list_json[0]["id"], project_a_id_str.as_str());

    RegisteredProject {
        id: project_a_id,
        id_str: project_a_id_str,
        host: project_a_host,
        dir: project_a_dir,
    }
}

/// Sub-step helper for Step 3: Start DevVM, Launch DSH, Verify Separate Statuses & Project URLs
async fn assert_step_3_vm_and_dsh_lifecycle(ctx: &AcceptanceContext, project: &RegisteredProject) {
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project.id);

    // 1. Explicitly Start DevVM
    let start_vm_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/start",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(start_vm_res.status(), ReqwestStatusCode::OK);

    // Status: VM is running, DSH is stopped
    let status_res1 = ctx.client.get(&proj_url).send().await.unwrap();
    let status_json1: Value = status_res1.json().await.unwrap();
    assert_eq!(status_json1["vm_status"], "running");
    assert_eq!(status_json1["dsh_status"], "stopped");
    assert!(status_json1["links"]["local_dsh_url"].is_null());
    assert!(status_json1["links"]["tailnet_dsh_url"].is_null());

    // 2. Launch DSH Runtime
    let launch_dsh_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_dsh_res.status(), ReqwestStatusCode::OK);

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Status: VM is running, DSH is running, and links are present
    let status_res2 = ctx.client.get(&proj_url).send().await.unwrap();
    let status_json2: Value = status_res2.json().await.unwrap();
    assert_eq!(status_json2["vm_status"], "running");
    assert_eq!(status_json2["dsh_status"], "running");

    let expected_local_dsh_url = format!(
        "http://3080.{}.devvm.localhost:{}",
        project.host, ctx.caddy_port
    );
    let expected_tailnet_dsh_url = format!(
        "http://3080.{}.devvm.internal:{}",
        project.host, ctx.caddy_port
    );
    assert_eq!(
        status_json2["links"]["local_dsh_url"].as_str().unwrap(),
        expected_local_dsh_url
    );
    assert_eq!(
        status_json2["links"]["tailnet_dsh_url"].as_str().unwrap(),
        expected_tailnet_dsh_url
    );

    // 3. Idempotent Launch DSH: repeated click keeps DSH running
    let launch_again_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_again_res.status(), ReqwestStatusCode::OK);

    let status_res3 = ctx.client.get(&proj_url).send().await.unwrap();
    let status_json3: Value = status_res3.json().await.unwrap();
    assert_eq!(status_json3["dsh_status"], "running");

    // 4. Stop DSH separately from VM
    let stop_dsh_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/stop",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stop_dsh_res.status(), ReqwestStatusCode::OK);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let status_res4 = ctx.client.get(&proj_url).send().await.unwrap();
    let status_json4: Value = status_res4.json().await.unwrap();
    assert_eq!(status_json4["vm_status"], "running");
    assert_eq!(status_json4["dsh_status"], "stopped");

    // 5. Stop DevVM
    let stop_vm_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/stop",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stop_vm_res.status(), ReqwestStatusCode::OK);

    let status_res5 = ctx.client.get(&proj_url).send().await.unwrap();
    let status_json5: Value = status_res5.json().await.unwrap();
    assert_eq!(status_json5["vm_status"], "stopped");
    assert_eq!(status_json5["dsh_status"], "stopped");

    // 6. Launch DSH when VM is stopped -> auto starts DevVM and launches DSH
    let launch_from_stopped = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_from_stopped.status(), ReqwestStatusCode::OK);

    tokio::time::sleep(Duration::from_millis(150)).await;

    let status_res6 = ctx.client.get(&proj_url).send().await.unwrap();
    let status_json6: Value = status_res6.json().await.unwrap();
    assert_eq!(status_json6["vm_status"], "running");
    assert_eq!(status_json6["dsh_status"], "running");
}

/// Sub-step helper for Step 4: Open Port Endpoint for Arbitrary Guest Port
async fn assert_step_4_open_port_endpoints(ctx: &AcceptanceContext, project: &RegisteredProject) {
    let open_port_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/open-port",
            ctx.server_addr, project.id
        ))
        .json(&json!({ "port": ctx.echo_port }))
        .send()
        .await
        .unwrap();
    assert_eq!(open_port_res.status(), ReqwestStatusCode::OK);
    let open_port_json: Value = open_port_res.json().await.unwrap();

    let expected_open_local = format!(
        "http://{}.{}.devvm.localhost:{}",
        ctx.echo_port, project.host, ctx.caddy_port
    );
    let expected_open_tailnet = format!(
        "http://{}.{}.devvm.internal:{}",
        ctx.echo_port, project.host, ctx.caddy_port
    );
    assert_eq!(open_port_json["local_url"], expected_open_local);
    assert_eq!(open_port_json["tailnet_url"], expected_open_tailnet);

    // Port 0 validation failure
    let invalid_port_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/open-port",
            ctx.server_addr, project.id
        ))
        .json(&json!({ "port": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_port_res.status(), ReqwestStatusCode::BAD_REQUEST);
}

/// Sub-step helper for Step 5: Real Caddy Loopback Facade & Ingress Routing
async fn assert_step_5_caddy_ingress_and_loopback_facade(
    ctx: &AcceptanceContext,
    project: &RegisteredProject,
) {
    let caddy_base_url = format!("http://127.0.0.1:{}/api/echo", ctx.caddy_port);

    // 1. Request via local Project URL Host & Origin -> rewritten to loopback
    let local_host_header = format!(
        "{}.{}.devvm.localhost:{}",
        ctx.echo_port, project.host, ctx.caddy_port
    );
    let local_origin_header = format!(
        "http://{}.{}.devvm.localhost:{}",
        ctx.echo_port, project.host, ctx.caddy_port
    );

    let caddy_local_res = ctx
        .client
        .get(&caddy_base_url)
        .header("Host", &local_host_header)
        .header("Origin", &local_origin_header)
        .send()
        .await
        .unwrap();
    assert_eq!(caddy_local_res.status(), ReqwestStatusCode::OK);
    let echo_headers: Value = caddy_local_res.json().await.unwrap();
    assert_eq!(echo_headers["host"], format!("localhost:{}", ctx.echo_port));
    assert_eq!(
        echo_headers["origin"],
        format!("http://localhost:{}", ctx.echo_port)
    );

    // 2. Request via Tailnet Project URL Host & Origin -> rewritten to loopback
    let tailnet_host_header = format!(
        "{}.{}.devvm.internal:{}",
        ctx.echo_port, project.host, ctx.caddy_port
    );
    let tailnet_origin_header = format!(
        "http://{}.{}.devvm.internal:{}",
        ctx.echo_port, project.host, ctx.caddy_port
    );

    let caddy_tailnet_res = ctx
        .client
        .get(&caddy_base_url)
        .header("Host", &tailnet_host_header)
        .header("Origin", &tailnet_origin_header)
        .send()
        .await
        .unwrap();
    assert_eq!(caddy_tailnet_res.status(), ReqwestStatusCode::OK);
    let echo_tailnet_headers: Value = caddy_tailnet_res.json().await.unwrap();
    assert_eq!(
        echo_tailnet_headers["host"],
        format!("localhost:{}", ctx.echo_port)
    );
    assert_eq!(
        echo_tailnet_headers["origin"],
        format!("http://localhost:{}", ctx.echo_port)
    );

    // 3. Request without Origin header preserves plain GET behavior
    let caddy_plain_res = ctx
        .client
        .get(&caddy_base_url)
        .header("Host", &local_host_header)
        .send()
        .await
        .unwrap();
    assert_eq!(caddy_plain_res.status(), ReqwestStatusCode::OK);
    let plain_headers: Value = caddy_plain_res.json().await.unwrap();
    assert_eq!(
        plain_headers["host"],
        format!("localhost:{}", ctx.echo_port)
    );
    assert!(plain_headers.get("origin").is_none());

    // 4. Invalid Host rejected by Caddy with 400 Bad Request
    let caddy_bad_res = ctx
        .client
        .get(&caddy_base_url)
        .header("Host", "attacker-injected-domain.com")
        .send()
        .await
        .unwrap();
    assert_eq!(caddy_bad_res.status(), ReqwestStatusCode::BAD_REQUEST);
}

/// Sub-step helper for Step 6: Real Wildcard DNS Server Resolution (*.devvm.internal)
async fn assert_step_6_dns_resolution(ctx: &AcceptanceContext, project: &RegisteredProject) {
    let dns_client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut dns_buf = vec![0u8; 1024];

    // Query 1: DSH port subdomain
    let dsh_dns_qname = format!("3080.{}.devvm.internal", project.host);
    let query1 = build_dns_query(0x2001, &dsh_dns_qname, 1);
    dns_client.send_to(&query1, ctx.dns_addr).await.unwrap();

    let (len1, _) =
        tokio::time::timeout(Duration::from_secs(2), dns_client.recv_from(&mut dns_buf))
            .await
            .expect("DNS response timeout")
            .unwrap();
    let resp1 = parse_dns_response(&dns_buf[..len1]);
    assert_eq!(resp1.tx_id, 0x2001);
    assert_eq!(resp1.rcode, 0); // NoError
    assert!(resp1.is_authoritative);
    assert_eq!(resp1.ancount, 1);
    assert_eq!(resp1.a_records, vec![Ipv4Addr::new(100, 64, 0, 42)]);

    // Query 2: Arbitrary guest port subdomain
    let guest_dns_qname = format!("{}.{}.devvm.internal", ctx.echo_port, project.host);
    let query2 = build_dns_query(0x2002, &guest_dns_qname, 1);
    dns_client.send_to(&query2, ctx.dns_addr).await.unwrap();

    let (len2, _) =
        tokio::time::timeout(Duration::from_secs(2), dns_client.recv_from(&mut dns_buf))
            .await
            .expect("DNS response timeout")
            .unwrap();
    let resp2 = parse_dns_response(&dns_buf[..len2]);
    assert_eq!(resp2.tx_id, 0x2002);
    assert_eq!(resp2.rcode, 0);
    assert_eq!(resp2.a_records, vec![Ipv4Addr::new(100, 64, 0, 42)]);

    // Query 3: Non-matching domain -> NXDomain (rcode 3)
    let query3 = build_dns_query(0x2003, "external.service.com", 1);
    dns_client.send_to(&query3, ctx.dns_addr).await.unwrap();

    let (len3, _) =
        tokio::time::timeout(Duration::from_secs(2), dns_client.recv_from(&mut dns_buf))
            .await
            .expect("DNS response timeout")
            .unwrap();
    let resp3 = parse_dns_response(&dns_buf[..len3]);
    assert_eq!(resp3.tx_id, 0x2003);
    assert_eq!(resp3.rcode, 3); // NXDomain
    assert_eq!(resp3.ancount, 0);
}

/// Sub-step helper for Step 7: Project Logs Retrieval
async fn assert_step_7_project_logs(ctx: &AcceptanceContext, project: &RegisteredProject) {
    let logs_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}/logs",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(logs_res.status(), ReqwestStatusCode::OK);
    let logs_json: Value = logs_res.json().await.unwrap();
    let logs_content = logs_json["logs"].as_str().unwrap();

    assert!(logs_content.contains("Invoking `devvm start`"));
    assert!(logs_content.contains("Mock DevVM: started"));
    assert!(logs_content.contains("Launching DSH Runtime inside DevVM"));
    assert!(logs_content.contains("dsh web: http://127.0.0.1:3080"));
}

/// Sub-step helper for Step 8: Sync Setup, Manual Sync, Startup Reconciliation, Degraded Sync
async fn assert_step_8_sync_reconciliation_and_degraded_modes(
    ctx: &AcceptanceContext,
    project: &RegisteredProject,
    workspace_dir: &Path,
) {
    // 1. Setup Sync configuration
    let sync_setup_res = ctx
        .client
        .post(format!("http://{}/api/sync/setup", ctx.server_addr))
        .json(&json!({
            "ssh_user": "vps-devvm",
            "ssh_host": "sync.tailnet.internal",
            "ssh_port": 22,
            "ssh_key_path": "/root/.ssh/id_ed25519",
            "remote_sync_root": "/var/lib/devvm-sync",
            "verify": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(sync_setup_res.status(), ReqwestStatusCode::OK);

    let sync_cfg_res = ctx
        .client
        .get(format!("http://{}/api/sync/config", ctx.server_addr))
        .send()
        .await
        .unwrap();
    assert_eq!(sync_cfg_res.status(), ReqwestStatusCode::OK);
    let sync_cfg_json: Value = sync_cfg_res.json().await.unwrap();
    assert_eq!(sync_cfg_json["configured"], true);
    assert_eq!(sync_cfg_json["config"]["ssh_user"], "vps-devvm");

    // 2. Trigger Manual Sync
    let manual_sync_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/trigger",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(manual_sync_res.status(), ReqwestStatusCode::OK);

    tokio::time::sleep(Duration::from_millis(50)).await;
    let synced_project: Value = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(synced_project["sync_status"], "synchronized");

    // 3. Clean Pull Startup Reconciliation
    let clean_proj_dir = workspace_dir.join("project-clean");
    fs::create_dir_all(&clean_proj_dir).unwrap();
    let reg_clean_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": clean_proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let clean_proj: Value = reg_clean_res.json().await.unwrap();
    let clean_id_str = clean_proj["id"].as_str().unwrap();

    let launch_clean_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, clean_id_str
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_clean_res.status(), ReqwestStatusCode::OK);

    let clean_status_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, clean_id_str
        ))
        .send()
        .await
        .unwrap();
    let clean_status_json: Value = clean_status_res.json().await.unwrap();
    assert_eq!(clean_status_json["sync_status"], "synchronized");

    // 4. Dirty Push Startup Reconciliation
    let dirty_proj_dir = workspace_dir.join("project-dirty");
    let dirty_dsh_dir = dirty_proj_dir.join(".dsh");
    fs::create_dir_all(dirty_dsh_dir.join("storages")).unwrap();
    fs::write(dirty_dsh_dir.join(".sync-dirty"), "1\n").unwrap();
    fs::write(dirty_dsh_dir.join("storages/workspace.json"), "{}\n").unwrap();

    let reg_dirty_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": dirty_proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let dirty_proj: Value = reg_dirty_res.json().await.unwrap();
    let dirty_id_str = dirty_proj["id"].as_str().unwrap();

    let launch_dirty_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, dirty_id_str
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_dirty_res.status(), ReqwestStatusCode::OK);
    assert!(
        !dirty_dsh_dir.join(".sync-dirty").exists(),
        "Dirty marker must be removed after successful sync"
    );
    let dirty_status: Value = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, dirty_id_str
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(dirty_status["sync_status"], "synchronized");

    // 5. Degraded Sync Startup Reconciliation (Local state exists, VPS unreachable)
    let degraded_proj_dir = workspace_dir.join("project-degraded");
    let degraded_dsh_dir = degraded_proj_dir.join(".dsh/sessions");
    fs::create_dir_all(&degraded_dsh_dir).unwrap();
    fs::write(degraded_dsh_dir.join("local-session.jsonl"), "{}\n").unwrap();

    let reg_degraded_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": degraded_proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let degraded_proj: Value = reg_degraded_res.json().await.unwrap();
    let degraded_id_str = degraded_proj["id"].as_str().unwrap();

    ctx.fake_runner.vps_reachable.store(false, Ordering::SeqCst);

    let launch_degraded_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, degraded_id_str
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_degraded_res.status(), ReqwestStatusCode::OK);

    let degraded_status_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, degraded_id_str
        ))
        .send()
        .await
        .unwrap();
    let degraded_status_json: Value = degraded_status_res.json().await.unwrap();
    assert_eq!(degraded_status_json["sync_status"], "degraded");
    assert_eq!(degraded_status_json["dsh_status"], "running");

    // 6. Blocked DSH Startup (Empty local state + VPS unreachable -> BLOCKED)
    let empty_proj_dir = workspace_dir.join("project-empty");
    fs::create_dir_all(&empty_proj_dir).unwrap();

    let reg_empty_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": empty_proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let empty_proj: Value = reg_empty_res.json().await.unwrap();
    let empty_id_str = empty_proj["id"].as_str().unwrap();

    let launch_empty_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, empty_id_str
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        launch_empty_res.status(),
        ReqwestStatusCode::INTERNAL_SERVER_ERROR
    );
    let empty_err: Value = launch_empty_res.json().await.unwrap();
    assert!(empty_err["error"]
        .as_str()
        .unwrap()
        .contains("preventing divergent empty history"));

    let empty_status_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, empty_id_str
        ))
        .send()
        .await
        .unwrap();
    let empty_status_json: Value = empty_status_res.json().await.unwrap();
    assert_eq!(empty_status_json["dsh_status"], "stopped");

    ctx.fake_runner.vps_reachable.store(true, Ordering::SeqCst);
}

/// Sub-step helper for Step 9: Separation of Unregister, Local VM Deletion, Confirmed Sync Store Deletion
async fn assert_step_9_lifecycle_separation_and_deletion(
    ctx: &AcceptanceContext,
    project: &RegisteredProject,
) {
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project.id);

    // 1. Unregister: Removes registry entry, leaves VM and project files intact
    let unreg_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/unregister",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unreg_res.status(), ReqwestStatusCode::OK);

    let list_after_unreg = ctx
        .client
        .get(format!("http://{}/api/projects", ctx.server_addr))
        .send()
        .await
        .unwrap();
    let list_after_unreg_json: Vec<Value> = list_after_unreg.json().await.unwrap();
    assert!(!list_after_unreg_json
        .iter()
        .any(|p| p["id"] == project.id_str));

    // Invariant: Unregister does NOT delete .devvm-id or project files or .vm_running
    assert!(project.dir.exists());
    assert!(project.dir.join(".devvm-id").exists());
    assert!(project.dir.join(".vm_running").exists());

    // 2. Re-register and test Local DevVM Deletion
    let rereg_for_del = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": project.dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(rereg_for_del.status(), ReqwestStatusCode::OK);

    let delete_vm_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/delete",
            ctx.server_addr, project.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(delete_vm_res.status(), ReqwestStatusCode::OK);

    let status_after_del = ctx.client.get(&proj_url).send().await.unwrap();
    let status_after_del_json: Value = status_after_del.json().await.unwrap();
    assert_eq!(status_after_del_json["vm_status"], "stopped");

    // Invariant: VM deletion does NOT delete project directory or .devvm-id
    assert!(project.dir.exists());
    assert!(project.dir.join(".devvm-id").exists());
    assert!(!project.dir.join(".vm_running").exists());

    // 3. Separate Confirmed Sync Store Deletion
    // Unconfirmed deletion -> 400 Bad Request
    let unconf_sync_del_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            ctx.server_addr, project.id
        ))
        .json(&json!({ "confirmed": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(unconf_sync_del_res.status(), ReqwestStatusCode::BAD_REQUEST);

    // Confirmed deletion -> succeeds
    let conf_sync_del_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            ctx.server_addr, project.id
        ))
        .json(&json!({ "confirmed": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(conf_sync_del_res.status(), ReqwestStatusCode::OK);

    // Invariant: Sync store deletion leaves local project files and registry entry intact
    assert!(project.dir.exists());
    assert!(project.dir.join(".devvm-id").exists());
    let get_final_res = ctx.client.get(&proj_url).send().await.unwrap();
    assert_eq!(get_final_res.status(), ReqwestStatusCode::OK);
}

#[tokio::test]
async fn test_acceptance_complete_version_one_workflow() {
    let ctx = setup_acceptance_system().await;

    let workspace_dir = ctx.home_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).unwrap();

    // STEP 1: Project Browser Jail Verification
    assert_step_1_browser_jail(&ctx, &workspace_dir).await;

    // STEP 2: Project Registration & UUID .devvm-id Lifecycle
    let project_a = assert_step_2_project_registration_and_id_lifecycle(&ctx, &workspace_dir).await;

    // STEP 3: Start DevVM, Launch DSH, Verify Separate Statuses & Project URLs
    assert_step_3_vm_and_dsh_lifecycle(&ctx, &project_a).await;

    // STEP 4: Open Port Endpoint for Arbitrary Guest Port
    assert_step_4_open_port_endpoints(&ctx, &project_a).await;

    // STEP 5: Real Caddy Loopback Facade & Ingress Routing
    assert_step_5_caddy_ingress_and_loopback_facade(&ctx, &project_a).await;

    // STEP 6: Real Wildcard DNS Server Resolution (*.devvm.internal)
    assert_step_6_dns_resolution(&ctx, &project_a).await;

    // STEP 7: Project Logs Retrieval
    assert_step_7_project_logs(&ctx, &project_a).await;

    // STEP 8: Sync Setup, Manual Sync, Startup Reconciliation, Degraded Sync
    assert_step_8_sync_reconciliation_and_degraded_modes(&ctx, &project_a, &workspace_dir).await;

    // STEP 9: Separation of Unregister, Local VM Deletion, Confirmed Sync Store Deletion
    assert_step_9_lifecycle_separation_and_deletion(&ctx, &project_a).await;
}
