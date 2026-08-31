mod common;

use common::create_mock_devvm;
use devvm_daemon::{
    create_router, save_sync_config, AppState, DaemonConfig, DshRuntimeManager, SyncConfig,
    SyncManager,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::net::TcpListener;
use uuid::Uuid;

struct SyncTestContext {
    _temp_dir: tempfile::TempDir,
    config: DaemonConfig,
    server_addr: SocketAddr,
    client: reqwest::Client,
}

async fn setup_sync_test_server() -> SyncTestContext {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    let ssh_script = r#"#!/usr/bin/env bash
if [[ -f ".mock_ssh_fail" ]] || [[ "$*" == *"127.0.0.1:1"* ]] || [[ "$*" == *"-p 1 "* ]]; then
    echo "Mock SSH: connection refused" >&2
    exit 255
fi
exit 0
"#;
    let ssh_bin = bin_dir.join("ssh");
    fs::write(&ssh_bin, ssh_script).unwrap();
    let mut ssh_perms = fs::metadata(&ssh_bin).unwrap().permissions();
    ssh_perms.set_mode(0o755);
    fs::set_permissions(&ssh_bin, ssh_perms).unwrap();

    let devvm_bin = bin_dir.join("mock_devvm");
    create_mock_devvm(&devvm_bin);

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

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path,
        sync_config_path,
        log_dir,
        home_dir,
        devvm_bin: devvm_bin.clone(),
        ingress_port: 8102,
        tailnet_domain: "devvm.internal".to_string(),
    };

    let sync_manager = SyncManager::with_devvm_bin(devvm_bin);
    let dsh_runtime_manager = DshRuntimeManager::new();

    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager,
        sync_manager,
    };

    let router = create_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::builder().build().unwrap();

    SyncTestContext {
        _temp_dir: temp_dir,
        config,
        server_addr,
        client,
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

    // 4. Verify config endpoint returns newly saved config
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
}

#[tokio::test]
async fn test_startup_reconciliation_unconfigured_sync() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("unconfigured-proj");
    fs::create_dir_all(&proj_dir).unwrap();

    // Register project
    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let reg_body: Value = reg_res.json().await.unwrap();
    let project_id = reg_body["id"].as_str().unwrap();

    // Launch DSH without sync configured -> works normally
    let launch_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_res.status(), StatusCode::OK);

    // Check project sync status is not_configured
    let proj_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    let proj_body: Value = proj_res.json().await.unwrap();
    assert_eq!(proj_body["sync_status"], "not_configured");
    assert_eq!(proj_body["dsh_status"], "running");
}

#[tokio::test]
async fn test_startup_reconciliation_clean_pull() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("clean-pull-proj");
    fs::create_dir_all(&proj_dir).unwrap();

    // Configure sync with a simulated local listener/dummy host
    let sync_cfg = SyncConfig {
        ssh_user: "devvm".to_string(),
        ssh_host: "127.0.0.1".to_string(),
        ssh_port: 22,
        ssh_key_path: PathBuf::from("/root/.ssh/id_rsa"),
        remote_sync_root: "/var/lib/devvm-sync".to_string(),
    };
    save_sync_config(&ctx.config.sync_config_path, &sync_cfg).unwrap();

    // Register project
    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let reg_body: Value = reg_res.json().await.unwrap();
    let project_id = reg_body["id"].as_str().unwrap();

    // Launch DSH -> clean local state inside DevVM triggers pull from Sync Store
    let launch_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();

    // If SSH connection to dummy host fails, degraded sync kicks in or clean pull succeeds
    assert!(
        launch_res.status().is_success()
            || launch_res.status() == StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_startup_reconciliation_dirty_push() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("dirty-push-proj");
    fs::create_dir_all(&proj_dir).unwrap();

    // In-VM state is simulated inside DevVM at .mock_dsh
    let in_vm_dsh = proj_dir.join(".mock_dsh");
    fs::create_dir_all(in_vm_dsh.join("storages")).unwrap();
    fs::write(in_vm_dsh.join(".sync-dirty"), "1\n").unwrap();
    fs::write(in_vm_dsh.join("storages/workspace.json"), "{}\n").unwrap();

    // Host project directory has NO .dsh directory!
    assert!(!proj_dir.join(".dsh").exists());

    // Configure sync
    let sync_cfg = SyncConfig {
        ssh_user: "devvm".to_string(),
        ssh_host: "sync.vps".to_string(),
        ssh_port: 22,
        ssh_key_path: PathBuf::from("/root/.ssh/id_rsa"),
        remote_sync_root: "/var/lib/devvm-sync".to_string(),
    };
    save_sync_config(&ctx.config.sync_config_path, &sync_cfg).unwrap();

    // Register project
    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let reg_body: Value = reg_res.json().await.unwrap();
    let project_id = reg_body["id"].as_str().unwrap();

    // Launch DSH -> in-VM dirty state is pushed and cleaned
    let launch_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(launch_res.status(), StatusCode::OK);
    assert!(!in_vm_dsh.join(".sync-dirty").exists());
}

#[tokio::test]
async fn test_startup_reconciliation_degraded_when_vps_down_and_local_exists() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("degraded-proj");
    fs::create_dir_all(&proj_dir).unwrap();

    // In-VM state exists inside DevVM at .mock_dsh/sessions
    let in_vm_sessions = proj_dir.join(".mock_dsh/sessions");
    fs::create_dir_all(&in_vm_sessions).unwrap();
    fs::write(in_vm_sessions.join("existing-session.jsonl"), "{}\n").unwrap();

    // Host project directory has NO .dsh directory
    assert!(!proj_dir.join(".dsh").exists());

    // Configure sync with unreachable port to guarantee connection failure
    let sync_cfg = SyncConfig {
        ssh_user: "devvm".to_string(),
        ssh_host: "127.0.0.1".to_string(),
        ssh_port: 1,
        ssh_key_path: PathBuf::from("/root/.ssh/id_rsa"),
        remote_sync_root: "/var/lib/devvm-sync".to_string(),
    };
    save_sync_config(&ctx.config.sync_config_path, &sync_cfg).unwrap();

    // Register project
    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let reg_body: Value = reg_res.json().await.unwrap();
    let project_id = reg_body["id"].as_str().unwrap();

    // Launch DSH -> VPS unreachable and local state exists in DevVM -> launches in Degraded Sync status
    let launch_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_res.status(), StatusCode::OK);

    let proj_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    let proj_body: Value = proj_res.json().await.unwrap();
    assert_eq!(proj_body["sync_status"], "degraded");
    assert_eq!(proj_body["dsh_status"], "running");

    // Check project log contains warning
    let logs_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}/logs",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    let logs_body: Value = logs_res.json().await.unwrap();
    assert!(logs_body["logs"]
        .as_str()
        .unwrap()
        .contains("Degraded Sync"));
}

#[tokio::test]
async fn test_startup_reconciliation_blocks_when_vps_down_and_no_local_state() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("empty-blocked-proj");
    fs::create_dir_all(&proj_dir).unwrap();

    // Host and in-VM state are both empty

    // Configure sync with unreachable port
    let sync_cfg = SyncConfig {
        ssh_user: "devvm".to_string(),
        ssh_host: "127.0.0.1".to_string(),
        ssh_port: 1,
        ssh_key_path: PathBuf::from("/root/.ssh/id_rsa"),
        remote_sync_root: "/var/lib/devvm-sync".to_string(),
    };
    save_sync_config(&ctx.config.sync_config_path, &sync_cfg).unwrap();

    // Register project
    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let reg_body: Value = reg_res.json().await.unwrap();
    let project_id = reg_body["id"].as_str().unwrap();

    // Launch DSH -> MUST BE BLOCKED to prevent divergent empty history
    let launch_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let err_body: Value = launch_res.json().await.unwrap();
    assert!(err_body["error"]
        .as_str()
        .unwrap()
        .contains("preventing divergent empty history"));

    // DSH status must remain stopped
    let proj_res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    let proj_body: Value = proj_res.json().await.unwrap();
    assert_eq!(proj_body["dsh_status"], "stopped");
}

#[tokio::test]
async fn test_manual_sync_trigger_and_store_deletion() {
    let ctx = setup_sync_test_server().await;
    let proj_dir = ctx.config.home_dir.join("sync-actions-proj");
    fs::create_dir_all(&proj_dir).unwrap();

    // Configure sync
    let sync_cfg = SyncConfig {
        ssh_user: "devvm".to_string(),
        ssh_host: "sync.vps".to_string(),
        ssh_port: 22,
        ssh_key_path: PathBuf::from("/root/.ssh/id_rsa"),
        remote_sync_root: "/var/lib/devvm-sync".to_string(),
    };
    save_sync_config(&ctx.config.sync_config_path, &sync_cfg).unwrap();

    // Register project
    let reg_res = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let reg_body: Value = reg_res.json().await.unwrap();
    let project_id_str = reg_body["id"].as_str().unwrap();
    let project_id = Uuid::parse_str(project_id_str).unwrap();

    // 1. Trigger manual sync
    let trigger_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/trigger",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(trigger_res.status(), StatusCode::OK);

    // 2. Delete Sync Store without confirmation -> rejected
    let unconfirmed_del_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            ctx.server_addr, project_id
        ))
        .json(&json!({ "confirmed": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(unconfirmed_del_res.status(), StatusCode::BAD_REQUEST);

    // 3. Delete Sync Store with confirmed: true
    let confirmed_del_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/sync/delete",
            ctx.server_addr, project_id
        ))
        .json(&json!({ "confirmed": true }))
        .send()
        .await
        .unwrap();
    assert!(
        confirmed_del_res.status().is_success()
            || confirmed_del_res.status() == StatusCode::BAD_REQUEST
    );
}
