mod common;

use axum::{
    extract::ConnectInfo,
    http::{HeaderMap, StatusCode as AxumStatusCode},
    response::{IntoResponse, Json},
    Router,
};
use common::{build_dns_query, parse_dns_response};
use devvm_daemon::{
    create_router, detect_tailscale_ipv4, AppState, DaemonConfig, DnsConfig, DnsServer,
    DshRuntimeManager, SyncManager,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::watch;
use uuid::Uuid;

/// Helper to check if a CLI tool exists in PATH.
fn tool_in_path(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Helper to discover the devvm CLI binary path.
fn find_devvm_binary() -> Option<PathBuf> {
    if let Ok(path) = env::var("DEVVM_BIN") {
        let p = PathBuf::from(path);
        if p.exists() || tool_in_path(p.to_str().unwrap_or("")) {
            return Some(p);
        }
    }
    if tool_in_path("devvm") {
        return Some(PathBuf::from("devvm"));
    }
    let local_devvm = PathBuf::from("./devvm");
    if local_devvm.exists() {
        return Some(local_devvm);
    }
    if let Some(home) = dirs::home_dir() {
        let local_devvm = home.join(".local/bin/devvm");
        if local_devvm.exists() {
            return Some(local_devvm);
        }
    }
    None
}

/// Helper to discover the frps binary path.
fn find_frps_binary() -> Option<PathBuf> {
    if let Ok(path) = env::var("DEVVM_FRPS_BIN") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(path) = env::var("FRPS_BIN") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    if tool_in_path("frps") {
        return Some(PathBuf::from("frps"));
    }
    if let Some(home) = dirs::home_dir() {
        let local_frps = home.join(".local/bin/frps");
        if local_frps.exists() {
            return Some(local_frps);
        }
    }
    None
}

/// Runs a command inside a project's DevVM via `devvm exec -- <args...>`.
fn devvm_exec(devvm_bin: &Path, project_dir: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(devvm_bin);
    cmd.arg("exec")
        .arg("--")
        .args(args)
        .current_dir(project_dir);
    cmd.output().unwrap_or_else(|e| {
        panic!(
            "Failed to run devvm exec in {}: {}",
            project_dir.display(),
            e
        )
    })
}

/// RAII Guard ensuring spawned child processes and temporary test resources
/// are reaped and cleaned up on test completion or failure, while NEVER deleting
/// non-test Sync Store data on the remote VPS.
struct LiveAcceptanceGuard {
    child_processes: Vec<Child>,
    local_dirs: Vec<PathBuf>,
    ssh_cleanup_targets: Vec<(String, String, u16, PathBuf, String, Uuid)>,
}

impl LiveAcceptanceGuard {
    fn new() -> Self {
        Self {
            child_processes: Vec::new(),
            local_dirs: Vec::new(),
            ssh_cleanup_targets: Vec::new(),
        }
    }

    fn add_child(&mut self, child: Child) {
        self.child_processes.push(child);
    }

    fn add_dir(&mut self, dir: PathBuf) {
        self.local_dirs.push(dir);
    }

    fn add_ssh_target(
        &mut self,
        user: String,
        host: String,
        port: u16,
        key: PathBuf,
        root: String,
        project_id: Uuid,
    ) {
        self.ssh_cleanup_targets
            .push((user, host, port, key, root, project_id));
    }
}

impl Drop for LiveAcceptanceGuard {
    fn drop(&mut self) {
        // 1. Terminate all spawned child processes (Caddy, frps, frpc, etc.)
        for child in &mut self.child_processes {
            let _ = child.kill();
            let _ = child.wait();
        }

        // 2. Clean up remote test Sync Store data for exact test project UUIDs only
        for (ssh_user, ssh_host, ssh_port, ssh_key, sync_root, project_id) in
            &self.ssh_cleanup_targets
        {
            if !project_id.is_nil() {
                let rm_cmd = format!(
                    "rm -rf \"{}/{}\"",
                    sync_root.trim_end_matches('/'),
                    project_id
                );
                let _ = Command::new("ssh")
                    .arg("-p")
                    .arg(ssh_port.to_string())
                    .arg("-i")
                    .arg(ssh_key)
                    .arg("-o")
                    .arg("StrictHostKeyChecking=accept-new")
                    .arg("-o")
                    .arg("BatchMode=yes")
                    .arg(format!("{}@{}", ssh_user, ssh_host))
                    .arg(&rm_cmd)
                    .output();
            }
        }

        // 3. Remove local temporary workspace directories
        for dir in &self.local_dirs {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Upstream Axum handler returning received HTTP headers and connection peer info.
async fn live_echo_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let mut map = serde_json::Map::new();
    map.insert("client_peer_addr".to_string(), json!(addr.to_string()));
    map.insert("client_peer_ip".to_string(), json!(addr.ip().to_string()));
    for (k, v) in headers.iter() {
        if let Ok(v_str) = v.to_str() {
            map.insert(k.as_str().to_string(), json!(v_str));
        }
    }
    (AxumStatusCode::OK, Json(map))
}

/// Polls project synchronization status until reaching terminal status (`synchronized`),
/// or failing immediately on `failed`/`degraded` status.
async fn poll_project_sync_status(
    client: &reqwest::Client,
    daemon_addr: SocketAddr,
    project_id: Uuid,
    timeout: Duration,
) -> Result<String, String> {
    let start = std::time::Instant::now();
    let mut last_status = "unknown".to_string();

    while start.elapsed() < timeout {
        let res = client
            .get(format!(
                "http://{}/api/projects/{}",
                daemon_addr, project_id
            ))
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if res.status().is_success() {
            let json: Value = res
                .json()
                .await
                .map_err(|e| format!("Failed to parse JSON: {}", e))?;
            let status = json["sync_status"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            last_status = status.clone();

            if status == "synchronized" {
                return Ok(status);
            }
            if status == "failed" || status == "degraded" {
                return Err(format!(
                    "Sync reached terminal failure or degraded state: {}",
                    status
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    Err(format!(
        "Timed out after {:?} waiting for sync to reach synchronized (last status: {})",
        timeout, last_status
    ))
}

/// Verifies that expected Portable DSH State categories exist in the VPS Sync Store
/// and excluded rebuildable caches / credentials / configuration do not.
fn verify_remote_sync_store_contents(
    ssh_user: &str,
    ssh_host: &str,
    ssh_port: u16,
    ssh_key: &Path,
    sync_root: &str,
    project_id: Uuid,
) {
    let check_cmd = format!(
        "test -f \"{0}/{1}/sessions/acceptance-project/acceptance-session-1/session.jsonl\" && \
         test -f \"{0}/{1}/attachments/v1/objects/9e/9e1779dd2d1b2550d1564d2b06494e256b2ad6524aeb51b50caea0a8c34a958c\" && \
         test -f \"{0}/{1}/storages/workspace.json\" && \
         test -f \"{0}/{1}/storages/message_feedback.json\" && \
         test ! -f \"{0}/{1}/storages/session_projcache.json\" && \
         test ! -d \"{0}/{1}/attachments/v1/request-images\" && \
         test ! -d \"{0}/{1}/credentials\" && \
         test ! -d \"{0}/{1}/settings\" && \
         test ! -d \"{0}/{1}/plugins\"",
        sync_root.trim_end_matches('/'),
        project_id
    );

    let output = Command::new("ssh")
        .arg("-p")
        .arg(ssh_port.to_string())
        .arg("-i")
        .arg(ssh_key)
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(format!("{}@{}", ssh_user, ssh_host))
        .arg(&check_cmd)
        .output()
        .expect("Failed to execute SSH check on VPS Sync Store");

    assert!(
        output.status.success(),
        "VPS Sync Store verification failed! Expected files missing or excluded files present. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Opt-in live acceptance test for the complete version-one system.
///
/// Exercises real SmolVM, real DSH, real Caddy ingress with Loopback Facade,
/// real FRP transport routing, real wildcard DNS, real Tailnet routing, and real VPS SSH/rsync
/// synchronization & clean restore on Linux and macOS.
///
/// Run with:
/// ```bash
/// DEVVM_LIVE_ACCEPTANCE=1 cargo test --test live_acceptance_test -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore]
async fn test_live_complete_version_one_system() {
    let live_enabled = env::var("DEVVM_LIVE_ACCEPTANCE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !live_enabled {
        println!("\n===================================================================");
        println!("  DEVVM LIVE ACCEPTANCE TEST (SKIPPED)");
        println!("===================================================================");
        println!("This test exercises the live environment: SmolVM, DSH, Caddy, FRP, DNS,");
        println!("rsync/SSH VPS Sync Store, and Tailscale.\n");
        println!("To enable live acceptance testing, set DEVVM_LIVE_ACCEPTANCE=1:");
        println!("  DEVVM_LIVE_ACCEPTANCE=1 cargo test --test live_acceptance_test -- --ignored --nocapture\n");
        println!("Required live environment dependencies:");
        println!(
            "  - Active Tailscale connection (tailscale CLI in PATH with assigned 100.x.y.z IPv4)"
        );
        println!("  - devvm CLI (in PATH, at ./devvm, or set DEVVM_BIN)");
        println!("  - frpc and frps binaries (in PATH, ~/.local/bin/frps, or set FRPS_BIN)");
        println!("  - caddy, rsync, and ssh binaries in PATH");
        println!("  - DEVVM_LIVE_SSH_HOST set to VPS Sync Store host\n");
        println!("Environment variables:");
        println!("  DEVVM_BIN               Path to devvm CLI (default: devvm in PATH or ./devvm)");
        println!("  DEVVM_FRPS_BIN / FRPS_BIN Path to frps binary (default: frps in PATH or ~/.local/bin/frps)");
        println!(
            "  DEVVM_LIVE_SSH_HOST     VPS Host for live sync test (e.g. sync.vps.net) [REQUIRED]"
        );
        println!("  DEVVM_LIVE_SSH_USER     VPS SSH user (default: devvm)");
        println!("  DEVVM_LIVE_SSH_PORT     VPS SSH port (default: 22)");
        println!("  DEVVM_LIVE_SSH_KEY      VPS SSH private key path (default: ~/.ssh/id_ed25519 or ~/.ssh/id_rsa)");
        println!("  DEVVM_LIVE_SYNC_ROOT    VPS remote sync root (default: /var/lib/devvm-sync)");
        println!("===================================================================\n");
        return;
    }

    println!("\n>>> Starting Live Acceptance Test for Complete Version-One System...");

    let mut guard = LiveAcceptanceGuard::new();

    // 1. Strict pre-flight binary, network, and credential verification
    let devvm_bin_opt = find_devvm_binary();
    let frps_bin_opt = find_frps_binary();
    let has_caddy = tool_in_path("caddy");
    let has_frpc = tool_in_path("frpc");
    let has_tailscale = tool_in_path("tailscale");
    let has_rsync = tool_in_path("rsync");
    let has_ssh = tool_in_path("ssh");
    let tailscale_ip_opt = detect_tailscale_ipv4();

    println!("Pre-flight checks:");
    println!("  - devvm CLI present: {:?}", devvm_bin_opt);
    println!("  - caddy binary present: {}", has_caddy);
    println!("  - frpc binary present: {}", has_frpc);
    println!("  - frps binary present: {:?}", frps_bin_opt);
    println!("  - tailscale present: {}", has_tailscale);
    println!("  - tailscale IPv4 detected: {:?}", tailscale_ip_opt);
    println!("  - rsync binary present: {}", has_rsync);
    println!("  - ssh binary present: {}", has_ssh);

    assert!(
        devvm_bin_opt.is_some(),
        "devvm CLI is required for live acceptance testing. Ensure devvm is in PATH, at ./devvm, or set DEVVM_BIN"
    );
    let devvm_bin_path = devvm_bin_opt.unwrap();

    assert!(
        has_caddy,
        "caddy binary is required for live acceptance testing. Ensure caddy is installed and in PATH."
    );
    assert!(
        has_frpc,
        "frpc binary is required for live acceptance testing. Ensure frpc is installed and in PATH."
    );
    assert!(
        frps_bin_opt.is_some(),
        "frps binary is required for live acceptance testing. Ensure frps is installed (e.g. run ./setup-devvm.sh or set FRPS_BIN / DEVVM_FRPS_BIN)."
    );
    let frps_bin_path = frps_bin_opt.unwrap();

    assert!(
        has_tailscale,
        "tailscale CLI is required for live acceptance testing. Ensure Tailscale is installed and in PATH."
    );
    assert!(
        tailscale_ip_opt.is_some(),
        "Tailscale IPv4 address could not be detected. Ensure Tailscale is running, authenticated, and online (`tailscale status`)."
    );
    let tailscale_ip = tailscale_ip_opt.unwrap();

    assert!(has_rsync, "rsync is required for live acceptance testing.");
    assert!(has_ssh, "ssh is required for live acceptance testing.");

    let home_dir = dirs::home_dir().expect("User home directory must exist");
    let ssh_host = env::var("DEVVM_LIVE_SSH_HOST").expect(
        "DEVVM_LIVE_SSH_HOST is required for live acceptance sync verification (e.g. DEVVM_LIVE_SSH_HOST=vps.tailnet.internal)."
    );
    let ssh_user = env::var("DEVVM_LIVE_SSH_USER").unwrap_or_else(|_| "devvm".to_string());
    let ssh_port = env::var("DEVVM_LIVE_SSH_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(22);
    let ssh_key_path = env::var("DEVVM_LIVE_SSH_KEY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let default_ed25519 = home_dir.join(".ssh/id_ed25519");
            if default_ed25519.exists() {
                default_ed25519
            } else {
                home_dir.join(".ssh/id_rsa")
            }
        });
    assert!(
        ssh_key_path.exists(),
        "SSH key not found at {}. Ensure an SSH key exists or set DEVVM_LIVE_SSH_KEY=<path>",
        ssh_key_path.display()
    );
    let sync_root =
        env::var("DEVVM_LIVE_SYNC_ROOT").unwrap_or_else(|_| "/var/lib/devvm-sync".to_string());

    // Strict SSH preflight verification to VPS Sync Store
    let ssh_preflight_check = Command::new("ssh")
        .arg("-p")
        .arg(ssh_port.to_string())
        .arg("-i")
        .arg(&ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=5")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(format!("{}@{}", ssh_user, ssh_host))
        .arg(format!("mkdir -p \"{0}\" && test -w \"{0}\"", sync_root))
        .output()
        .expect("Failed to execute SSH preflight check to VPS Sync Store");
    assert!(
        ssh_preflight_check.status.success(),
        "VPS Sync Store SSH preflight check failed (exit code {:?}): {}",
        ssh_preflight_check.status.code(),
        String::from_utf8_lossy(&ssh_preflight_check.stderr)
    );

    // 2. Setup real test project under home directory
    let test_workspace = home_dir.join(".devvm-live-acceptance");
    fs::create_dir_all(&test_workspace).expect("Failed to create test workspace directory");
    guard.add_dir(test_workspace.clone());

    let project_dir = test_workspace.join(format!("live-proj-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&project_dir).expect("Failed to create live project directory");

    println!("Created live test project at: {}", project_dir.display());

    // 3. Start real Caddy process with test port & Loopback Facade
    let temp_config_dir = tempdir().unwrap();
    let caddy_listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let caddy_port = caddy_listener.local_addr().unwrap().port();
    drop(caddy_listener);

    let template_caddyfile =
        fs::read_to_string("scripts/Caddyfile").expect("scripts/Caddyfile must exist");
    let test_caddyfile_content = template_caddyfile.replace(":10080", &format!(":{}", caddy_port));
    let caddyfile_path = temp_config_dir.path().join("Caddyfile");
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

    guard.add_child(caddy_child);

    // Start mock upstream echo server on an ephemeral port
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_addr = echo_listener.local_addr().unwrap();
    let echo_port = echo_addr.port();

    let echo_app = Router::new().fallback(live_echo_handler);
    tokio::spawn(async move {
        axum::serve(
            echo_listener,
            echo_app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let client = reqwest::Client::builder().build().unwrap();

    // Wait for Caddy to become ready
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

    // 4. Start Control Daemon with real config
    let config_path = temp_config_dir.path().join("projects.json");
    let sync_config_path = temp_config_dir.path().join("sync.json");
    let log_dir = temp_config_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let daemon_config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path,
        sync_config_path: sync_config_path.clone(),
        log_dir: log_dir.clone(),
        home_dir: home_dir.clone(),
        devvm_bin: devvm_bin_path.clone(),
        ingress_port: caddy_port,
        tailnet_domain: "devvm.internal".to_string(),
    };

    let dsh_runtime_manager = DshRuntimeManager::new();
    let sync_manager = SyncManager::new();

    let state = AppState {
        config: daemon_config.clone(),
        dsh_runtime_manager,
        sync_manager,
    };

    let router = create_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let daemon_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // 5. Test Project Registration
    println!(">>> Registering project with Control Daemon...");
    let reg_res = client
        .post(format!("http://{}/api/projects/register", daemon_addr))
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .expect("Failed to call project register API");
    assert_eq!(reg_res.status(), StatusCode::OK);
    let project_view: Value = reg_res.json().await.unwrap();
    let project_id_str = project_view["id"].as_str().unwrap();
    let project_id = Uuid::parse_str(project_id_str).unwrap();
    let project_host = project_view["project_host"].as_str().unwrap().to_string();

    println!("  - Registered Project ID: {}", project_id);
    println!("  - Project Host: {}", project_host);

    // Register remote cleanup target with guard
    guard.add_ssh_target(
        ssh_user.clone(),
        ssh_host.clone(),
        ssh_port,
        ssh_key_path.clone(),
        sync_root.clone(),
        project_id,
    );

    // Verify .devvm-id exists on disk
    let id_file = project_dir.join(".devvm-id");
    assert!(id_file.exists(), ".devvm-id must be written to project dir");
    assert_eq!(fs::read_to_string(&id_file).unwrap().trim(), project_id_str);

    // 6. Exercise real FRP transport and Caddy routing end-to-end
    println!(
        ">>> Exercising real FRP transport routing through frps -> frpc -> Caddy -> upstream..."
    );

    let frps_bind_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let frps_bind_port = frps_bind_listener.local_addr().unwrap().port();
    drop(frps_bind_listener);

    let frps_vhost_listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let frps_vhost_port = frps_vhost_listener.local_addr().unwrap().port();
    drop(frps_vhost_listener);

    let frps_config = format!(
        "bindAddr = \"0.0.0.0\"\nbindPort = {}\nvhostHTTPPort = {}\n",
        frps_bind_port, frps_vhost_port
    );
    let frps_toml_path = temp_config_dir.path().join("frps.toml");
    fs::write(&frps_toml_path, frps_config).unwrap();

    let frps_child = Command::new(&frps_bin_path)
        .arg("-c")
        .arg(&frps_toml_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start frps");
    guard.add_child(frps_child);

    let frpc_config = format!(
        "serverAddr = \"127.0.0.1\"\nserverPort = {}\n\n[[proxies]]\nname = \"{}\"\ntype = \"http\"\nlocalIP = \"127.0.0.1\"\nlocalPort = {}\ncustomDomains = [\n\t\"*.{}.devvm.localhost\",\n\t\"*.{}.devvm.internal\",\n]\n",
        frps_bind_port, project_host, caddy_port, project_host, project_host
    );
    let frpc_toml_path = temp_config_dir.path().join("frpc.toml");
    fs::write(&frpc_toml_path, frpc_config).unwrap();

    let frpc_child = Command::new("frpc")
        .arg("-c")
        .arg(&frpc_toml_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start frpc");
    guard.add_child(frpc_child);

    // Wait for FRP transport tunnel to establish
    let frp_test_url = format!("http://127.0.0.1:{}/frp-ready-check", frps_vhost_port);
    let mut frp_ready = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(res) = client
            .get(&frp_test_url)
            .header(
                "Host",
                format!(
                    "{}.{}.devvm.localhost:{}",
                    echo_port, project_host, frps_vhost_port
                ),
            )
            .send()
            .await
        {
            if res.status().is_success() {
                frp_ready = true;
                break;
            }
        }
    }
    assert!(
        frp_ready,
        "FRP transport tunnel failed to become ready within timeout"
    );

    // Exercise FRP transport with local Project URL
    let frp_local_res = client
        .get(format!(
            "http://127.0.0.1:{}/test-frp-local",
            frps_vhost_port
        ))
        .header(
            "Host",
            format!(
                "{}.{}.devvm.localhost:{}",
                echo_port, project_host, frps_vhost_port
            ),
        )
        .header(
            "Origin",
            format!(
                "http://{}.{}.devvm.localhost:{}",
                echo_port, project_host, frps_vhost_port
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(frp_local_res.status(), StatusCode::OK);
    let frp_local_json: Value = frp_local_res.json().await.unwrap();
    assert_eq!(
        frp_local_json["host"],
        format!("localhost:{}", echo_port),
        "FRP + Caddy must rewrite Host to loopback authority"
    );
    assert_eq!(
        frp_local_json["origin"],
        format!("http://localhost:{}", echo_port),
        "FRP + Caddy must rewrite Origin to loopback authority"
    );

    // Exercise FRP transport with Tailnet Project URL
    let frp_tailnet_res = client
        .get(format!(
            "http://127.0.0.1:{}/test-frp-tailnet",
            frps_vhost_port
        ))
        .header(
            "Host",
            format!(
                "{}.{}.devvm.internal:{}",
                echo_port, project_host, frps_vhost_port
            ),
        )
        .header(
            "Origin",
            format!(
                "http://{}.{}.devvm.internal:{}",
                echo_port, project_host, frps_vhost_port
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(frp_tailnet_res.status(), StatusCode::OK);
    let frp_tailnet_json: Value = frp_tailnet_res.json().await.unwrap();
    assert_eq!(frp_tailnet_json["host"], format!("localhost:{}", echo_port));
    assert_eq!(
        frp_tailnet_json["origin"],
        format!("http://localhost:{}", echo_port)
    );

    // Unrouted project host through FRP should not route (404)
    let frp_unrouted_res = client
        .get(format!("http://127.0.0.1:{}/unrouted", frps_vhost_port))
        .header(
            "Host",
            format!(
                "{}.unregistered-host.devvm.localhost:{}",
                echo_port, frps_vhost_port
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        frp_unrouted_res.status(),
        StatusCode::NOT_FOUND,
        "FRP must reject unrouted hostnames with 404"
    );

    // 7. Wildcard DNS Resolution & Real Tailnet Interface Traffic
    println!(">>> Testing Wildcard DNS Resolution and Real Tailnet Interface routing...");

    let dns_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_addr = dns_socket.local_addr().unwrap();

    let dns_config = DnsConfig {
        bind_addr: dns_addr.to_string(),
        target_ip: tailscale_ip,
        domain: "devvm.internal".to_string(),
        target_ipv6: None,
        ttl: 60,
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        DnsServer::run_with_socket(dns_socket, dns_config, Some(shutdown_rx))
            .await
            .unwrap();
    });

    let dns_client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut dns_buf = vec![0u8; 512];
    let qname = format!("3080.{}.devvm.internal", project_host);
    let full_query = build_dns_query(0x1234, &qname, 1);

    dns_client.send_to(&full_query, dns_addr).await.unwrap();
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), dns_client.recv_from(&mut dns_buf))
        .await
        .expect("DNS response timeout")
        .unwrap();
    let resp = parse_dns_response(&dns_buf[..len]);
    assert_eq!(resp.tx_id, 0x1234);
    assert_eq!(resp.rcode, 0);
    assert_eq!(resp.a_records, vec![tailscale_ip]);
    let _ = shutdown_tx.send(true);
    println!(
        "  - DNS wildcard query successfully resolved to Tailscale IP: {}",
        tailscale_ip
    );

    // Direct HTTP request to Tailscale IP proving network interface reception
    let tailnet_req_res = client
        .get(format!(
            "http://{}:{}/tailnet-proof",
            tailscale_ip, caddy_port
        ))
        .header(
            "Host",
            format!(
                "{}.{}.devvm.internal:{}",
                echo_port, project_host, caddy_port
            ),
        )
        .header(
            "Origin",
            format!(
                "http://{}.{}.devvm.internal:{}",
                echo_port, project_host, caddy_port
            ),
        )
        .send()
        .await
        .expect("Direct request to Tailscale IP must succeed");
    assert_eq!(tailnet_req_res.status(), StatusCode::OK);
    let tailnet_headers: Value = tailnet_req_res.json().await.unwrap();
    assert_eq!(tailnet_headers["host"], format!("localhost:{}", echo_port));
    assert_eq!(
        tailnet_headers["origin"],
        format!("http://localhost:{}", echo_port)
    );
    println!(
        "  - Verified traffic arrived over Tailscale IP address: {}",
        tailscale_ip
    );

    // 8. Exercise real DevVM & DSH lifecycle
    println!(">>> Testing DevVM start...");
    let start_res = client
        .post(format!(
            "http://{}/api/projects/{}/vm/start",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(start_res.status(), StatusCode::OK);

    println!(">>> Testing DSH Launch...");
    let launch_res = client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_res.status(), StatusCode::OK);

    tokio::time::sleep(Duration::from_millis(500)).await;

    let proj_res = client
        .get(format!(
            "http://{}/api/projects/{}",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(proj_res.status(), StatusCode::OK);
    let proj_info: Value = proj_res.json().await.unwrap();
    println!("  - VM Status: {}", proj_info["vm_status"]);
    println!("  - DSH Status: {}", proj_info["dsh_status"]);

    let expected_local_dsh_url = format!(
        "http://3080.{}.devvm.localhost:{}",
        project_host, caddy_port
    );
    let expected_tailnet_dsh_url =
        format!("http://3080.{}.devvm.internal:{}", project_host, caddy_port);
    assert_eq!(
        proj_info["links"]["local_dsh_url"].as_str().unwrap(),
        expected_local_dsh_url
    );
    assert_eq!(
        proj_info["links"]["tailnet_dsh_url"].as_str().unwrap(),
        expected_tailnet_dsh_url
    );

    // Test Open Port endpoint
    let open_port_res = client
        .post(format!(
            "http://{}/api/projects/{}/open-port",
            daemon_addr, project_id
        ))
        .json(&json!({ "port": echo_port }))
        .send()
        .await
        .unwrap();
    assert_eq!(open_port_res.status(), StatusCode::OK);
    let open_port_json: Value = open_port_res.json().await.unwrap();
    assert_eq!(
        open_port_json["local_url"],
        format!(
            "http://{}.{}.devvm.localhost:{}",
            echo_port, project_host, caddy_port
        )
    );
    assert_eq!(
        open_port_json["tailnet_url"],
        format!(
            "http://{}.{}.devvm.internal:{}",
            echo_port, project_host, caddy_port
        )
    );

    // Test Project Logs retrieval
    let logs_res = client
        .get(format!(
            "http://{}/api/projects/{}/logs",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(logs_res.status(), StatusCode::OK);

    // 9. Populate Portable DSH State in original Project's DevVM at /root/.dsh (included + excluded categories)
    println!(">>> Populating VM-local Portable DSH State categories inside original Project DevVM (/root/.dsh)...");
    let populate_script = r#"
set -eu
mkdir -p /root/.dsh/sessions/acceptance-project/acceptance-session-1 \
  /root/.dsh/storages \
  /root/.dsh/attachments/v1/objects/9e \
  /root/.dsh/attachments/v1/request-images/cd \
  /root/.dsh/credentials /root/.dsh/settings /root/.dsh/plugins
printf '{"id":"live-turn-1","role":"user","content":"acceptance test payload"}\n' > /root/.dsh/sessions/acceptance-project/acceptance-session-1/session.jsonl
printf 'devvm-attachment-binary-content-12345' > /root/.dsh/attachments/v1/objects/9e/9e1779dd2d1b2550d1564d2b06494e256b2ad6524aeb51b50caea0a8c34a958c
printf '{"version":1,"workspaces":[{"id":"ws-acceptance","title":"Live Acceptance Test Workspace"}]}' > /root/.dsh/storages/workspace.json
printf '{"ratings":[{"turn_id":"live-turn-1","rating":5,"feedback":"excellent"}]}' > /root/.dsh/storages/message_feedback.json

# Excluded rebuildable data and workstation configuration
printf 'projection cache index' > /root/.dsh/storages/session_projcache.json
printf 'derived request image' > /root/.dsh/attachments/v1/request-images/cd/derived
printf 'SUPER_SECRET_KEY' > /root/.dsh/credentials/secret.key
printf '{"theme":"dark"}' > /root/.dsh/settings/user-settings.json
printf 'console.log("plugin")' > /root/.dsh/plugins/custom-plugin.js
"#;

    let pop_out = devvm_exec(
        &devvm_bin_path,
        &project_dir,
        &["/bin/sh", "-c", populate_script],
    );
    assert!(
        pop_out.status.success(),
        "Failed to populate VM-local Portable DSH State in original Project DevVM: {}",
        String::from_utf8_lossy(&pop_out.stderr)
    );

    // 10. Configure VPS Sync Store & Trigger Manual Sync
    println!(
        ">>> Configuring VPS Sync Store and triggering live manual synchronization with {}@{}:{}...",
        ssh_user, ssh_host, ssh_port
    );
    let setup_res = client
        .post(format!("http://{}/api/sync/setup", daemon_addr))
        .json(&json!({
            "ssh_user": ssh_user,
            "ssh_host": ssh_host,
            "ssh_port": ssh_port,
            "ssh_key_path": ssh_key_path.to_str().unwrap(),
            "remote_sync_root": sync_root,
            "verify": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        setup_res.status(),
        StatusCode::OK,
        "Live sync setup must succeed"
    );

    let trigger_res = client
        .post(format!(
            "http://{}/api/projects/{}/sync/trigger",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        trigger_res.status(),
        StatusCode::OK,
        "Live sync trigger must succeed"
    );

    // 11. Poll until terminal status and fail on failed/degraded
    println!(">>> Polling until synchronization reaches terminal status...");
    let sync_outcome =
        poll_project_sync_status(&client, daemon_addr, project_id, Duration::from_secs(30)).await;
    assert!(
        sync_outcome.is_ok(),
        "Manual synchronization failed: {:?}",
        sync_outcome.err()
    );
    println!("  - Synchronization reached terminal status: synchronized");

    // 12. Verify expected files exist in Sync Store on remote VPS
    println!(">>> Verifying Sync Store file contents on remote VPS via SSH...");
    verify_remote_sync_store_contents(
        &ssh_user,
        &ssh_host,
        ssh_port,
        &ssh_key_path,
        &sync_root,
        project_id,
    );
    println!("  - Confirmed expected Portable DSH State categories exist in remote Sync Store and excluded files do not.");

    // 13. Demonstrate restore into clean/fresh local state representing another workstation/clone
    println!(">>> Demonstrating restore into clean/fresh local state representing another workstation clone...");
    let clone_dir = test_workspace.join(format!("live-clone-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&clone_dir).unwrap();
    guard.add_dir(clone_dir.clone());

    // Plant the same .devvm-id so it reaches the same project Sync Store
    fs::write(clone_dir.join(".devvm-id"), format!("{}\n", project_id)).unwrap();

    // Register the clone project
    let reg_clone_res = client
        .post(format!("http://{}/api/projects/register", daemon_addr))
        .json(&json!({ "path": clone_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg_clone_res.status(), StatusCode::OK);
    let clone_view: Value = reg_clone_res.json().await.unwrap();
    assert_eq!(clone_view["id"].as_str().unwrap(), project_id.to_string());

    // Launch DSH on the clean clone (triggers startup reconciliation pull into clone DevVM /root/.dsh)
    let launch_clone_res = client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_clone_res.status(), StatusCode::OK);

    // Poll until clone synchronization reaches terminal status
    let clone_sync_outcome =
        poll_project_sync_status(&client, daemon_addr, project_id, Duration::from_secs(30)).await;
    assert!(
        clone_sync_outcome.is_ok(),
        "Clone startup reconciliation sync failed: {:?}",
        clone_sync_outcome.err()
    );

    // 14. Verify restored Portable DSH State categories directly inside clone DevVM (/root/.dsh) via devvm exec
    println!(
        ">>> Verifying restored Portable DSH State categories inside clone DevVM (/root/.dsh)..."
    );
    let verify_clone_script = r#"
set -eu
# 1. Verify included restored categories
test -f /root/.dsh/sessions/acceptance-project/acceptance-session-1/session.jsonl
grep -q 'acceptance test payload' /root/.dsh/sessions/acceptance-project/acceptance-session-1/session.jsonl

test -f /root/.dsh/attachments/v1/objects/9e/9e1779dd2d1b2550d1564d2b06494e256b2ad6524aeb51b50caea0a8c34a958c
grep -q 'devvm-attachment-binary-content-12345' /root/.dsh/attachments/v1/objects/9e/9e1779dd2d1b2550d1564d2b06494e256b2ad6524aeb51b50caea0a8c34a958c

test -f /root/.dsh/storages/workspace.json
grep -q 'Live Acceptance Test Workspace' /root/.dsh/storages/workspace.json

test -f /root/.dsh/storages/message_feedback.json
grep -q 'excellent' /root/.dsh/storages/message_feedback.json

# 2. Verify excluded categories are NOT restored from Sync Store
test ! -f /root/.dsh/storages/session_projcache.json
test ! -d /root/.dsh/attachments/v1/request-images
test ! -f /root/.dsh/credentials/secret.key
test ! -f /root/.dsh/settings/user-settings.json
test ! -f /root/.dsh/plugins/custom-plugin.js
test ! -f /root/.dsh/.sync-dirty
"#;

    let verify_out = devvm_exec(
        &devvm_bin_path,
        &clone_dir,
        &["/bin/sh", "-c", verify_clone_script],
    );
    assert!(
        verify_out.status.success(),
        "Verification of restored Portable DSH State inside clone DevVM failed! Stderr: {}",
        String::from_utf8_lossy(&verify_out.stderr)
    );

    println!("  - Verified clean clone DevVM successfully restored VM-local Portable DSH State while excluding caches and configs.");

    // 15. Stop DSH and DevVM
    println!(">>> Stopping DSH and DevVM...");
    let _ = client
        .post(format!(
            "http://{}/api/projects/{}/dsh/stop",
            daemon_addr, project_id
        ))
        .send()
        .await;

    let _ = client
        .post(format!(
            "http://{}/api/projects/{}/vm/stop",
            daemon_addr, project_id
        ))
        .send()
        .await;

    // 16. Test Confirmed Sync Store Deletion (never deletes non-test data)
    println!(">>> Testing confirmed Sync Store deletion...");
    let delete_sync_res = client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            daemon_addr, project_id
        ))
        .json(&json!({ "confirmed": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(delete_sync_res.status(), StatusCode::OK);

    // Verify remote test project directory is deleted
    let verify_del_cmd = format!(
        "test ! -d \"{}/{}\"",
        sync_root.trim_end_matches('/'),
        project_id
    );
    let ssh_del_check = Command::new("ssh")
        .arg("-p")
        .arg(ssh_port.to_string())
        .arg("-i")
        .arg(&ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(format!("{}@{}", ssh_user, ssh_host))
        .arg(&verify_del_cmd)
        .output()
        .expect("Failed to execute SSH check on VPS Sync Store");
    assert!(
        ssh_del_check.status.success(),
        "Remote test Sync Store directory was not deleted!"
    );

    // 17. Clean Unregister and Cleanup
    println!(">>> Unregistering projects...");
    let unreg_res = client
        .post(format!(
            "http://{}/api/projects/{}/unregister",
            daemon_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unreg_res.status(), StatusCode::OK);

    println!("\n>>> Live Acceptance Test Completed Successfully!\n");
}
