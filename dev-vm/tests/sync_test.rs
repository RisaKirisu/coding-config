mod common;

use common::{
    devvm_status_output, exited, is_devvm, is_guest_exec, log_entries_text, ready, verify,
    MockRunner, DEVVM, DSH_STATUS,
};
use devvm_daemon::{
    create_router, load_sync_config, save_sync_config, AppState, CommandSpec, DaemonConfig,
    SyncConfig,
};
use mockall::Sequence;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
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
    server: JoinHandle<()>,
    runner: Arc<MockRunner>,
    previous_devvm_root: Option<OsString>,
}

async fn setup_sync_test_server(
    build_runner: impl FnOnce(&DaemonConfig) -> MockRunner,
) -> SyncTestContext {
    let env_guard = ENV_GUARD.lock().await;
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let devvm_root = temp_dir.path().join("devvm_root");
    fs::create_dir_all(&devvm_root).unwrap();
    let previous_devvm_root = std::env::var_os("DEVVM_ROOT");
    std::env::set_var("DEVVM_ROOT", &devvm_root);

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
        devvm_bin: PathBuf::from(DEVVM),
        ingress_port: 8102,
        remote_domain: None,
    };

    let runner = Arc::new(build_runner(&config));
    let router = create_router(AppState::new(config.clone(), runner.clone()));
    let server = tokio::spawn(async move {
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
        server,
        runner,
        previous_devvm_root,
    }
}

impl SyncTestContext {
    async fn finish(self) {
        self.server.abort();
        let _ = self.server.await;
        verify(self.runner);
        match self.previous_devvm_root {
            Some(value) => std::env::set_var("DEVVM_ROOT", value),
            None => std::env::remove_var("DEVVM_ROOT"),
        }
    }

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

fn expected_ssh(config: &SyncConfig, remote_command: String) -> CommandSpec {
    CommandSpec {
        program: PathBuf::from("ssh"),
        args: vec![
            "-p".into(),
            config.ssh_port.to_string(),
            "-i".into(),
            config.ssh_key_path.display().to_string(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-o".into(),
            "ConnectTimeout=5".into(),
            "-o".into(),
            "BatchMode=yes".into(),
            format!("{}@{}", config.ssh_user, config.ssh_host),
            remote_command,
        ],
        current_dir: None,
    }
}

fn expect_status(
    runner: &mut MockRunner,
    sequence: &mut Sequence,
    dir: &Path,
    state: &'static str,
    count: usize,
) {
    let dir = dir.to_owned();
    runner
        .expect_run()
        .withf(move |command, _| is_devvm(command, &dir, &["status"]))
        .times(count)
        .in_sequence(sequence)
        .returning(move |_, _| ready(devvm_status_output(state)));
}

fn expect_running_view(
    runner: &mut MockRunner,
    sequence: &mut Sequence,
    dir: &Path,
    sync_output: &'static str,
) {
    expect_status(runner, sequence, dir, "running", 1);
    let project_dir = dir.to_owned();
    runner
        .expect_run()
        .withf(move |command, _| is_guest_exec(command, &project_dir, DSH_STATUS))
        .times(1)
        .in_sequence(sequence)
        .returning(|_, _| ready(exited(1, "", "")));
    let project_dir = dir.to_owned();
    runner
        .expect_run()
        .withf(move |command, _| {
            is_devvm(
                command,
                &project_dir,
                &[
                    "exec",
                    "/bin/sh",
                    "-c",
                    "cat /run/devvm/sync-status.json 2>/dev/null",
                ],
            )
        })
        .times(1)
        .in_sequence(sequence)
        .returning(move |_, _| ready(exited(0, sync_output, "")));
    let project_dir = dir.to_owned();
    runner
        .expect_run()
        .withf(move |command, _| is_devvm(command, &project_dir, &["memory"]))
        .times(1)
        .in_sequence(sequence)
        .returning(|_, _| {
            ready(exited(
                0,
                r#"{"host_bytes":null,"guest_bytes":null,"limit_bytes":null}"#,
                "",
            ))
        });
}

#[tokio::test]
async fn test_sync_setup_and_config_endpoints() {
    let ctx = setup_sync_test_server(|config| {
        let mut runner = MockRunner::new();
        let mut sequence = Sequence::new();
        let mut sync = test_sync_config();
        sync.ssh_host = "127.0.0.1".into();
        sync.ssh_port = 1;
        sync.ssh_key_path = config.home_dir.join("id_rsa");
        let expected = expected_ssh(
            &sync,
            "mkdir -p '/var/lib/devvm-sync' && test -w '/var/lib/devvm-sync'".into(),
        );
        runner
            .expect_run()
            .withf(move |command, _| command == &expected)
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _| {
                ready(exited(
                    255,
                    "",
                    "ssh: connect to host 127.0.0.1 port 1: Connection refused\n",
                ))
            });
        runner
    })
    .await;

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
            "ssh_key_path": ctx.config.home_dir.join("id_rsa"),
            "remote_sync_root": "/var/lib/devvm-sync",
            "verify": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_setup_res.status(), StatusCode::BAD_REQUEST);
    let error: Value = bad_setup_res.json().await.unwrap();
    let error = error["error"].as_str().unwrap();
    assert!(
        error.contains("SSH connection check failed (exit code Some(255))"),
        "{error}"
    );
    assert!(error.contains("Connection refused"), "{error}");
    assert!(!ctx.config.sync_config_path.exists());

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
    ctx.finish().await;
}

#[tokio::test]
async fn test_project_view_reports_guest_sync_status_only_while_vm_runs() {
    let ctx = setup_sync_test_server(|config| {
        let dir = config.home_dir.join("status-proj");
        let mut runner = MockRunner::new();
        let mut sequence = Sequence::new();
        // Registration and the stopped view each probe status, without any guest command.
        expect_status(&mut runner, &mut sequence, &dir, "stopped", 2);
        let project_dir = dir.clone();
        runner.expect_run()
            .withf(move |command, _| is_devvm(command, &project_dir, &["start"]))
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _| ready(exited(0, "", "")));
        expect_running_view(&mut runner, &mut sequence, &dir, r#"{"status":"remote_ahead","head_seq":7,"last_error":null,"updated_at":"2024-05-01T10:00:00Z"}"#);
        expect_running_view(&mut runner, &mut sequence, &dir, "not json");
        runner
    }).await;
    let proj_dir = ctx.config.home_dir.join("status-proj");
    fs::create_dir_all(&proj_dir).unwrap();
    save_sync_config(&ctx.config.sync_config_path, &test_sync_config()).unwrap();
    let project_id = ctx.register_project(&proj_dir).await;

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

    let garbage_view = ctx.project_view(&project_id).await;
    assert!(garbage_view.get("sync_status").is_none());
    ctx.finish().await;
}

#[tokio::test]
async fn test_sync_store_deletion_requires_confirmation() {
    let ctx = setup_sync_test_server(|config| {
        let dir = config.home_dir.join("delete-store-proj");
        fs::create_dir_all(&dir).unwrap();
        let project =
            devvm_daemon::registry::register_project(&config.config_path, dir.to_str().unwrap())
                .unwrap();
        let mut runner = MockRunner::new();
        let mut sequence = Sequence::new();
        expect_status(&mut runner, &mut sequence, &dir, "stopped", 1);
        // The status probe follows the unconfirmed request, so a premature SSH command fails.
        expect_status(&mut runner, &mut sequence, &dir, "stopped", 1);
        let expected = expected_ssh(
            &test_sync_config(),
            format!("rm -rf '/var/lib/devvm-sync/{}'", project.id),
        );
        runner
            .expect_run()
            .withf(move |command, _| command == &expected)
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _| ready(exited(0, "", "")));
        runner
    })
    .await;
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
    assert_eq!(ctx.project_view(&project_id).await["vm_status"], "stopped");

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
    ctx.finish().await;
}
