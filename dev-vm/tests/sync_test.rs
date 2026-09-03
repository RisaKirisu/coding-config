mod common;

use common::{create_mock_devvm, create_mock_ssh, log_entries_text};
use devvm_daemon::{
    create_router, load_sync_config, save_sync_config, AppState, DaemonConfig, DshRuntimeManager,
    SyncConfig, SyncManager,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use tokio::net::TcpListener;
use uuid::Uuid;

/// `provision_sync_setup` resolves the guest config directory from the process-wide
/// `DEVVM_ROOT`, so tests that provision must not overlap.
static ENV_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct SyncTestContext {
    _env_guard: tokio::sync::MutexGuard<'static, ()>,
    _temp_dir: tempfile::TempDir,
    config: DaemonConfig,
    devvm_root: PathBuf,
    server_addr: SocketAddr,
    client: reqwest::Client,
}

async fn setup_sync_test_server() -> SyncTestContext {
    let env_guard = ENV_GUARD.lock().await;
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    create_mock_ssh(&bin_dir.join("ssh"));

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let devvm_bin = bin_dir.join("mock_devvm");
    create_mock_devvm(&devvm_bin, &log_dir);

    let devvm_root = temp_dir.path().join("devvm_root");
    fs::create_dir_all(&devvm_root).unwrap();
    std::env::set_var("DEVVM_ROOT", &devvm_root);

    let path_env = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    std::env::set_var("PATH", &path_env);

    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("projects.json");
    let sync_config_path = config_dir.join("sync.json");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: server_addr.port(),
        config_path,
        sync_config_path,
        log_dir,
        home_dir,
        devvm_bin: devvm_bin.clone(),
        ingress_port: 8102,
        tailnet_domain: "devvm.internal".to_string(),
    };

    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager: DshRuntimeManager::new(),
        sync_manager: SyncManager::with_devvm_bin(devvm_bin),
    };

    let router = create_router(state);
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::builder().build().unwrap();

    SyncTestContext {
        _env_guard: env_guard,
        _temp_dir: temp_dir,
        config,
        devvm_root,
        server_addr,
        client,
    }
}

impl SyncTestContext {
    async fn register_project(&self, dir: &Path) -> String {
        let res = self
            .client
            .post(format!("http://{}/api/projects/register", self.server_addr))
            .json(&json!({ "path": dir.to_str().unwrap() }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body: Value = res.json().await.unwrap();
        body["id"].as_str().unwrap().to_string()
    }

    async fn project_view(&self, project_id: &str) -> Value {
        self.client
            .get(format!(
                "http://{}/api/projects/{}",
                self.server_addr, project_id
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    fn guest_config_path(&self) -> PathBuf {
        self.devvm_root.join(".config/devvm/sync.json")
    }
}

fn test_sync_config() -> SyncConfig {
    SyncConfig {
        ssh_user: "devvm".to_string(),
        ssh_host: "sync.vps".to_string(),
        ssh_port: 22,
        ssh_key_path: PathBuf::from("/root/.ssh/id_rsa"),
        remote_sync_root: "/var/lib/devvm-sync".to_string(),
        writer_id: None,
        daemon_url: None,
    }
}

#[tokio::test]
async fn test_sync_setup_and_config_endpoints() {
    let ctx = setup_sync_test_server().await;

    // 1. Initial config should be unconfigured
    let res = ctx
        .client
        .get(format!("http://{}/api/sync/config", ctx.server_addr))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["configured"], false);

    // 2. Setup with unreachable host when verification is enabled (verify: true)
    let bad_setup_res = ctx
        .client
        .post(format!("http://{}/api/sync/setup", ctx.server_addr))
        .json(&json!({
            "ssh_user": "devvm",
            "ssh_host": "127.0.0.1",
            "ssh_port": 1,
            "ssh_key_path": "/root/.ssh/id_rsa",
            "remote_sync_root": "/var/lib/devvm-sync",
            "verify": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_setup_res.status(), StatusCode::BAD_REQUEST);

    // 3. Setup with verify: false
    let good_setup_res = ctx
        .client
        .post(format!("http://{}/api/sync/setup", ctx.server_addr))
        .json(&json!({
            "ssh_user": "devvm-user",
            "ssh_host": "sync.vps.net",
            "ssh_port": 2222,
            "ssh_key_path": "/root/.ssh/id_ed25519",
            "remote_sync_root": "/data/sync",
            "verify": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(good_setup_res.status(), StatusCode::OK);

    // 4. Config endpoint returns the newly saved HOST config
    let res2 = ctx
        .client
        .get(format!("http://{}/api/sync/config", ctx.server_addr))
        .send()
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body2: Value = res2.json().await.unwrap();
    assert_eq!(body2["configured"], true);
    assert_eq!(body2["config"]["ssh_user"], "devvm-user");
    assert_eq!(body2["config"]["ssh_host"], "sync.vps.net");
    assert_eq!(body2["config"]["ssh_port"], 2222);

    // 5. The GUEST config carries writer_id and daemon_url for the plugin
    let guest = load_sync_config(&ctx.guest_config_path()).unwrap().unwrap();
    let writer_id = guest.writer_id.clone().expect("guest writer_id must exist");
    Uuid::parse_str(&writer_id).expect("writer_id must be a UUID");
    assert_eq!(
        guest.daemon_url.as_deref(),
        Some(format!("http://127.0.0.1:{}", ctx.config.port).as_str())
    );

    // 6. A second setup preserves the same writer_id
    let second_setup_res = ctx
        .client
        .post(format!("http://{}/api/sync/setup", ctx.server_addr))
        .json(&json!({
            "ssh_user": "devvm-user",
            "ssh_host": "sync.vps.net",
            "ssh_port": 2222,
            "ssh_key_path": "/root/.ssh/id_ed25519",
            "remote_sync_root": "/data/sync",
            "verify": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(second_setup_res.status(), StatusCode::OK);
    let guest2 = load_sync_config(&ctx.guest_config_path()).unwrap().unwrap();
    assert_eq!(guest2.writer_id, Some(writer_id));
}

#[tokio::test]
async fn test_project_view_reports_guest_sync_status_only_while_vm_runs() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("status-proj");
    fs::create_dir_all(&proj_dir).unwrap();
    save_sync_config(&ctx.config.sync_config_path, &test_sync_config()).unwrap();

    let project_id = ctx.register_project(&proj_dir).await;

    // The plugin's status file inside the DevVM, mapped by the mock devvm.
    let guest_run = proj_dir.join(".mock_run");
    fs::create_dir_all(&guest_run).unwrap();
    fs::write(
        guest_run.join("sync-status.json"),
        r#"{"status":"remote_ahead","head_seq":7,"last_error":null,"updated_at":"2024-05-01T10:00:00Z"}"#,
    )
    .unwrap();

    // A stopped DevVM must never be probed: `devvm exec` would start one.
    let stopped_view = ctx.project_view(&project_id).await;
    assert_eq!(stopped_view["vm_status"], "stopped");
    assert!(
        stopped_view.get("sync_status").is_none(),
        "sync_status must be absent while the DevVM is stopped"
    );

    let start_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/start",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(start_res.status(), StatusCode::OK);

    let running_view = ctx.project_view(&project_id).await;
    assert_eq!(running_view["vm_status"], "running");
    assert_eq!(running_view["sync_status"], "remote_ahead");

    // Garbage in the status file leaves the badge off entirely.
    fs::write(guest_run.join("sync-status.json"), "not json").unwrap();
    let garbage_view = ctx.project_view(&project_id).await;
    assert!(garbage_view.get("sync_status").is_none());
}

#[tokio::test]
async fn test_sync_store_deletion_requires_confirmation() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("delete-store-proj");
    fs::create_dir_all(&proj_dir).unwrap();
    save_sync_config(&ctx.config.sync_config_path, &test_sync_config()).unwrap();

    let project_id = ctx.register_project(&proj_dir).await;

    let unconfirmed = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            ctx.server_addr, project_id
        ))
        .json(&json!({ "confirmed": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(unconfirmed.status(), StatusCode::BAD_REQUEST);

    let confirmed = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            ctx.server_addr, project_id
        ))
        .json(&json!({ "confirmed": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);

    let logs: Value = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}/logs",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(log_entries_text(&logs).contains("Remote Sync Store data deleted successfully."));
}

#[tokio::test]
async fn test_read_status_parses_every_contract_status() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("read-status-proj");
    let guest_run = proj_dir.join(".mock_run");
    fs::create_dir_all(&guest_run).unwrap();

    let manager = SyncManager::with_devvm_bin(ctx.config.devvm_bin.clone());
    let status_file = guest_run.join("sync-status.json");

    for status in [
        "not_configured",
        "synchronizing",
        "synchronized",
        "remote_ahead",
        "degraded",
        "failed",
    ] {
        fs::write(
            &status_file,
            format!(
                r#"{{"status":"{}","head_seq":1,"last_error":null,"updated_at":"2024-05-01T10:00:00Z"}}"#,
                status
            ),
        )
        .unwrap();
        let read = manager.read_status(&proj_dir).await.expect("must parse");
        assert_eq!(
            serde_json::to_value(read).unwrap(),
            Value::String(status.to_string())
        );
    }

    fs::write(&status_file, "}}} not json {{{").unwrap();
    assert!(manager.read_status(&proj_dir).await.is_none());

    fs::write(&status_file, r#"{"status":"unknown_future_status"}"#).unwrap();
    assert!(manager.read_status(&proj_dir).await.is_none());

    fs::remove_file(&status_file).unwrap();
    assert!(manager.read_status(&proj_dir).await.is_none());
}
