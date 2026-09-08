mod common;

use common::{
    create_mock_devvm, log_entries_text, mock_dsh_pid_file, mock_dsh_start_count,
    mock_dsh_token_file,
};
use devvm_daemon::{
    create_router, logs::dsh_token_path, AppState, DaemonConfig, DshRuntimeManager, SyncManager,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::TcpListener;
use uuid::Uuid;

struct TestContext {
    _temp_dir: tempfile::TempDir,
    home_dir: PathBuf,
    config: DaemonConfig,
    server_addr: SocketAddr,
    client: reqwest::Client,
}

/// Serves a router over an existing configuration, as a restarted daemon would.
async fn spawn_daemon(config: &DaemonConfig) -> SocketAddr {
    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager: DshRuntimeManager::new(),
        sync_manager: SyncManager::new(),
    };
    let router = create_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    addr
}

async fn register(ctx: &TestContext, project_dir: &std::path::Path) -> Uuid {
    let project: Value = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    project["id"].as_str().unwrap().parse().unwrap()
}

/// DSH is started detached, so its status becomes observable a moment after the launch call.
async fn wait_for_dsh_status(ctx: &TestContext, proj_url: &str, expected: &str) {
    for _ in 0..100 {
        let data: Value = ctx
            .client
            .get(proj_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if data["dsh_status"] == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("DSH never reached status {expected}");
}

async fn wait_for_dsh_token(project_dir: &std::path::Path) -> String {
    let token_file = mock_dsh_token_file(project_dir);
    for _ in 0..100 {
        if let Ok(token) = fs::read_to_string(&token_file) {
            let trimmed = token.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("token file never appeared at {token_file:?}");
}

async fn wait_for_dsh_link(ctx: &TestContext, proj_url: &str) -> Value {
    for _ in 0..100 {
        let data: Value = ctx
            .client
            .get(proj_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if data["dsh_status"] == "running" && !data["links"]["local_dsh_url"].is_null() {
            return data;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("DSH link never appeared for {proj_url}");
}

async fn wait_for_dsh_log(ctx: &TestContext, project_id: Uuid) -> String {
    let path = ctx
        .config
        .log_dir
        .join(project_id.to_string())
        .join("dsh.log");
    for _ in 0..100 {
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("dsh web: http://127.0.0.1:3080") {
                return content;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("dsh.log never received the DSH startup line at {path:?}");
}

/// Matches `[<ISO-8601 UTC with milliseconds>] ` at the start of a guest-written log line.
fn has_iso_prefix(line: &str) -> bool {
    let Some((timestamp, _)) = line.split_once("] ") else {
        return false;
    };
    let Some(timestamp) = timestamp.strip_prefix('[') else {
        return false;
    };
    let shape = "0000-00-00T00:00:00.000Z";
    timestamp.len() == shape.len()
        && timestamp.chars().zip(shape.chars()).all(|(c, s)| match s {
            '0' => c.is_ascii_digit(),
            other => c == other,
        })
}

async fn setup_test_server() -> TestContext {
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("projects.json");

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let devvm_bin = temp_dir.path().join("mock_devvm");
    create_mock_devvm(&devvm_bin, &log_dir);

    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path: config_path.clone(),
        sync_config_path: config_dir.join("sync.json"),
        log_dir: log_dir.clone(),
        home_dir: home_dir.clone(),
        devvm_bin: devvm_bin.clone(),
        ingress_port: 8102,
        remote_domain: Some("risak.dev".to_string()),
    };

    let sync_manager = SyncManager::new();
    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager: DshRuntimeManager::new(),
        sync_manager,
    };

    let router = create_router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::builder().build().unwrap();

    TestContext {
        _temp_dir: temp_dir,
        home_dir,
        config,
        server_addr,
        client,
    }
}

#[tokio::test]
async fn test_embedded_ui_served() {
    let ctx = setup_test_server().await;
    let url = format!("http://{}", ctx.server_addr);
    let res = ctx.client.get(&url).send().await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.text().await.unwrap();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("DevVM Control Daemon"));
    assert!(body.contains("fetchProjects"));
    assert!(body.contains("openProjectPort"));
    assert!(body.contains("--bg-color: #f8fafc"));
    assert!(body.contains("@keyframes spin"));
    assert!(body.contains("pendingActions"));
    assert!(body.contains("setInterval(refreshCurrentLogs, 2000)"));
    assert!(body.contains("clearInterval(logRefreshTimer)"));
    assert!(body.contains("VM: ${vmStatus.label}"));
    assert!(body.contains("DSH: ${dshStatus.label}"));
    assert!(body.contains(r#"data-source="daemon""#));
    assert!(body.contains(r#"data-source="dsh""#));
    assert!(body.contains(r#"data-source="ingress""#));
    assert!(body.contains("errors only"));
    assert!(body.contains("Follow"));
    assert!(body.contains("scrollTop"));
    assert!(body.contains("<= 24"));
}

#[tokio::test]
async fn test_project_logs_never_return_the_dsh_token_file() {
    let ctx = setup_test_server().await;
    let project_dir = ctx.home_dir.join("token-log-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let project_id = register(&ctx, &project_dir).await;

    let log_dir = ctx
        ._temp_dir
        .path()
        .join("logs")
        .join(project_id.to_string());
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(
        log_dir.join("dsh.log"),
        "[2026-03-03T01:15:17.000Z] dsh web: http://127.0.0.1:3080/?token=mock-token-redacted-1-base64url-auth-token00\n",
    )
    .unwrap();
    fs::write(
        log_dir.join("dsh.token"),
        "mock-token-redacted-1-base64url-auth-token00\n",
    )
    .unwrap();

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

    let entries = logs["entries"].as_array().unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry["message"].as_str().unwrap_or_default().contains("mock-token-redacted"))
            .count(),
        1,
        "the dsh.log startup line is a log entry, but the token file must never be read into the Project Log: {entries:?}"
    );
}

#[tokio::test]
async fn test_project_logs_merge_three_sources_in_time_order() {
    let ctx = setup_test_server().await;
    let project_dir = ctx.home_dir.join("merged-logs-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let project_id = register(&ctx, &project_dir).await;

    let log_dir = ctx
        ._temp_dir
        .path()
        .join("logs")
        .join(project_id.to_string());
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(
        log_dir.join("daemon.log"),
        "[2026-03-03T01:15:18.000Z] [daemon] Invoking `devvm start`\n",
    )
    .unwrap();
    fs::write(
        log_dir.join("dsh.log"),
        "[2026-03-03T01:15:17.000Z] dsh web: http://127.0.0.1:3080\n",
    )
    .unwrap();
    fs::write(
        log_dir.join("ingress.log"),
        "{\"level\":\"error\",\"ts\":1772500516.21,\"logger\":\"http.log.access\",\"msg\":\"handled request\",\"request\":{\"method\":\"GET\",\"uri\":\"/api/events.mux\"},\"status\":502,\"duration\":0.000203}\n",
    )
    .unwrap();

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

    let entries = logs["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "expected one entry per file: {entries:?}");
    let sources: Vec<&str> = entries
        .iter()
        .map(|entry| entry["source"].as_str().unwrap())
        .collect();
    assert_eq!(
        sources,
        vec!["ingress", "dsh", "daemon"],
        "entries must be ordered by timestamp, not by file: {entries:?}"
    );
    assert_eq!(
        entries[0]["message"].as_str().unwrap(),
        "GET /api/events.mux → 502 (0.2 ms)"
    );
    assert_eq!(entries[0]["level"].as_str().unwrap(), "error");
    assert_eq!(
        entries[0]["ts"].as_str().unwrap(),
        "2026-03-03T01:15:16.210Z"
    );
}

#[tokio::test]
async fn test_project_browser_jail() {
    let ctx = setup_test_server().await;

    // Create directories in home
    let proj1 = ctx.home_dir.join("work").join("project-a");
    fs::create_dir_all(&proj1).unwrap();

    let proj2 = ctx.home_dir.join("work").join("project-b");
    fs::create_dir_all(&proj2).unwrap();

    // Create directory outside home
    let outside = ctx._temp_dir.path().join("outside_dir");
    fs::create_dir_all(&outside).unwrap();

    let base_url = format!("http://{}/api/browser", ctx.server_addr);

    // 1. Root browser (default to home)
    let res = ctx.client.get(&base_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let data: Value = res.json().await.unwrap();
    assert_eq!(
        data["current"].as_str().unwrap(),
        fs::canonicalize(&ctx.home_dir).unwrap().to_str().unwrap()
    );
    let entries = data["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "work");

    // 2. Browse subfolder
    let work_dir = ctx.home_dir.join("work");
    let res = ctx
        .client
        .get(format!("{}?path={}", base_url, work_dir.to_str().unwrap()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let data: Value = res.json().await.unwrap();
    let entries = data["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);

    // 3. Attempt to browse outside home jail -> 403 Forbidden
    let res = ctx
        .client
        .get(format!("{}?path={}", base_url, outside.to_str().unwrap()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 4. Directory traversal attempting to escape -> 403 Forbidden
    let res = ctx
        .client
        .get(format!(
            "{}?path={}/../../",
            base_url,
            ctx.home_dir.to_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_project_registration_and_id_lifecycle() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("awesome-app");
    fs::create_dir_all(&project_dir).unwrap();

    let reg_url = format!("http://{}/api/projects/register", ctx.server_addr);
    let projects_url = format!("http://{}/api/projects", ctx.server_addr);

    // 1. Initial registration creates .devvm-id
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let project_view: Value = res.json().await.unwrap();
    let project_id_str = project_view["id"].as_str().unwrap();
    let project_uuid = Uuid::parse_str(project_id_str).unwrap();

    // Verify .devvm-id file exists and matches
    let id_file = project_dir.join(".devvm-id");
    assert!(id_file.exists());
    let written_id = fs::read_to_string(&id_file).unwrap();
    assert_eq!(written_id.trim(), project_id_str);

    // 2. Re-registering reuses existing .devvm-id
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let project_view2: Value = res.json().await.unwrap();
    assert_eq!(project_view2["id"].as_str().unwrap(), project_id_str);

    // 3. List projects
    let res = ctx.client.get(&projects_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: Vec<Value> = res.json().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], project_id_str);
    assert_eq!(list[0]["name"], "awesome-app");
    assert_eq!(list[0]["vm_status"], "stopped");
    assert_eq!(list[0]["dsh_status"], "stopped");
    assert_eq!(
        list[0]["links"]["local_port_template"],
        format!(
            "http://{{port}}.{}.devvm.localhost:8102",
            list[0]["project_host"].as_str().unwrap()
        )
    );
    assert_eq!(
        list[0]["links"]["tailnet_port_template"],
        format!(
            "https://{}-{{port}}.risak.dev",
            list[0]["project_host"].as_str().unwrap()
        )
    );

    // 4. Unregister project
    let unreg_url = format!(
        "http://{}/api/projects/{}/unregister",
        ctx.server_addr, project_uuid
    );
    let res = ctx.client.post(&unreg_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify removed from registry
    let res = ctx.client.get(&projects_url).send().await.unwrap();
    let list: Vec<Value> = res.json().await.unwrap();
    assert_eq!(list.len(), 0);

    // Invariant: Unregister does NOT delete .devvm-id or project files
    assert!(id_file.exists());
    assert!(project_dir.exists());
}

#[tokio::test]
async fn test_vm_lifecycle_operations() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("vm-test-proj");
    fs::create_dir_all(&project_dir).unwrap();

    let reg_url = format!("http://{}/api/projects/register", ctx.server_addr);
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let p: Value = res.json().await.unwrap();
    let project_id = p["id"].as_str().unwrap();

    // 1. Initial status: stopped
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project_id);
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["vm_status"], "stopped");

    // 2. Start VM
    let start_url = format!(
        "http://{}/api/projects/{}/vm/start",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&start_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify VM is now running
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["vm_status"], "running");

    // 3. Stop VM
    let stop_url = format!(
        "http://{}/api/projects/{}/vm/stop",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&stop_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify VM is now stopped
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["vm_status"], "stopped");

    // 4. Delete VM
    let del_url = format!(
        "http://{}/api/projects/{}/vm/delete",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&del_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Check logs contain daemon & devvm activity
    let logs_url = format!(
        "http://{}/api/projects/{}/logs",
        ctx.server_addr, project_id
    );
    let res = ctx.client.get(&logs_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let logs_data: Value = res.json().await.unwrap();
    let logs = log_entries_text(&logs_data);
    assert!(logs.contains("Invoking `devvm start`"));
    assert!(logs.contains("Mock DevVM: started"));
    assert!(logs.contains("Invoking `devvm stop`"));
    assert!(logs.contains("Invoking `devvm rm`"));
}

#[tokio::test]
async fn test_dsh_status_is_read_from_the_devvm_and_survives_a_daemon_restart() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("dsh-test-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let project_id = register(&ctx, &project_dir).await;
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project_id);

    // 1. Launch DSH while the DevVM is stopped: the DevVM is started first.
    let launch_url = format!(
        "http://{}/api/projects/{}/dsh/launch",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&launch_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    wait_for_dsh_status(&ctx, &proj_url, "running").await;
    let data = wait_for_dsh_link(&ctx, &proj_url).await;
    assert_eq!(data["vm_status"], "running");
    let expected_token = "mock-token-redacted-1-base64url-auth-token00";
    let expected_local_url = format!(
        "http://3080.{}.devvm.localhost:8102?token={}",
        data["project_host"].as_str().unwrap(),
        expected_token
    );
    let expected_tailnet_url = format!(
        "https://{}-3080.risak.dev?token={}",
        data["project_host"].as_str().unwrap(),
        expected_token
    );
    // The links must carry the token emitted by the running DSH's startup URL, not a
    // placeholder or an unauthenticated URL.
    assert!(data["links"]["local_dsh_url"]
        .as_str()
        .unwrap()
        .contains(expected_token));
    assert!(data["links"]["tailnet_dsh_url"]
        .as_str()
        .unwrap()
        .contains(expected_token));
    assert!(data["links"]["dsh_url"]
        .as_str()
        .unwrap()
        .contains(expected_token));
    assert_eq!(
        data["links"]["local_dsh_url"].as_str().unwrap(),
        expected_local_url
    );
    assert_eq!(
        data["links"]["tailnet_dsh_url"].as_str().unwrap(),
        expected_tailnet_url
    );
    assert_eq!(
        data["links"]["dsh_url"].as_str().unwrap(),
        expected_local_url
    );

    // A fresh manager over the same config reads the running DSH and the same token.
    let fresh_manager = DshRuntimeManager::new();
    let fresh_status = fresh_manager
        .get_status(&ctx.config, project_id, &project_dir)
        .await;
    assert_eq!(format!("{:?}", fresh_status), "Running");
    assert_eq!(
        fresh_manager.get_token(&ctx.config, project_id).as_deref(),
        Some(expected_token)
    );
    assert_eq!(
        fs::read_to_string(dsh_token_path(&ctx.config.log_dir, project_id))
            .unwrap()
            .trim(),
        expected_token,
        "the daemon must persist the token in the Project runtime directory, not only in guest /tmp"
    );

    // 3. Same through the HTTP API served by a restarted daemon.
    let restarted_addr = spawn_daemon(&ctx.config).await;
    let restarted: Value = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            restarted_addr, project_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(restarted["dsh_status"], "running");
    assert_eq!(
        restarted["links"]["local_dsh_url"].as_str().unwrap(),
        expected_local_url
    );
    assert_eq!(
        restarted["links"]["tailnet_dsh_url"].as_str().unwrap(),
        expected_tailnet_url
    );
    assert_eq!(
        restarted["links"]["dsh_url"].as_str().unwrap(),
        expected_local_url
    );

    // 4. dsh.log is written inside the DevVM with the ISO-8601 line prefix.
    let dsh_log = wait_for_dsh_log(&ctx, project_id).await;
    assert!(
        dsh_log.lines().all(has_iso_prefix),
        "every dsh.log line must carry the ISO prefix: {dsh_log}"
    );
    assert!(dsh_log.contains("dsh web: http://127.0.0.1:3080"));
    assert!(dsh_log.contains("startup reconciliation done"));

    // 5. The Project Log viewer shows the guest-written dsh.log.
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
    assert!(
        logs["entries"].as_array().unwrap().iter().any(|entry| {
            entry["source"] == "dsh" && !entry["ts"].as_str().unwrap_or_default().is_empty()
        }),
        "expected timestamped dsh entries: {}",
        logs["entries"]
    );

    // 6. Stop DSH: the guest process is gone and every manager reports stopped.
    let stop_dsh_url = format!(
        "http://{}/api/projects/{}/dsh/stop",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&stop_dsh_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let data: Value = ctx
        .client
        .get(&proj_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "stopped");
    assert!(data["links"]["local_dsh_url"].is_null());
    assert!(data["links"]["tailnet_dsh_url"].is_null());
    assert!(data["links"]["dsh_url"].is_null());

    let fresh_status = DshRuntimeManager::new()
        .get_status(&ctx.config, project_id, &project_dir)
        .await;
    assert_eq!(format!("{:?}", fresh_status), "Stopped");
    assert!(!mock_dsh_pid_file(&project_dir).exists());
    assert!(!mock_dsh_token_file(&project_dir).exists());
    assert!(!dsh_token_path(&ctx.config.log_dir, project_id).exists());
}

#[tokio::test]
async fn test_second_launch_does_not_spawn_a_second_dsh_process() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("dsh-idempotent-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let project_id = register(&ctx, &project_dir).await;
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project_id);
    let launch_url = format!(
        "http://{}/api/projects/{}/dsh/launch",
        ctx.server_addr, project_id
    );

    assert_eq!(
        ctx.client.post(&launch_url).send().await.unwrap().status(),
        StatusCode::OK
    );
    wait_for_dsh_status(&ctx, &proj_url, "running").await;
    let pid_after_first = fs::read_to_string(mock_dsh_pid_file(&project_dir)).unwrap();
    let token_after_first = wait_for_dsh_token(&project_dir).await;

    assert_eq!(
        ctx.client.post(&launch_url).send().await.unwrap().status(),
        StatusCode::OK
    );
    let data: Value = ctx
        .client
        .get(&proj_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "running");
    assert_eq!(
        mock_dsh_start_count(&project_dir),
        1,
        "an idempotent launch must not start a second DSH"
    );
    assert_eq!(
        fs::read_to_string(mock_dsh_pid_file(&project_dir)).unwrap(),
        pid_after_first
    );
    assert_eq!(
        fs::read_to_string(mock_dsh_token_file(&project_dir))
            .unwrap()
            .trim(),
        token_after_first,
        "an idempotent launch must not replace the token"
    );
    assert_eq!(
        fs::read_to_string(dsh_token_path(&ctx.config.log_dir, project_id))
            .unwrap()
            .trim(),
        token_after_first,
        "a redundant launch must not wipe the host token copy while DSH is alive"
    );

    assert_eq!(
        ctx.client
            .post(format!(
                "http://{}/api/projects/{}/dsh/stop",
                ctx.server_addr, project_id
            ))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_dsh_restart_replaces_the_running_process() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("dsh-restart-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let project_id = register(&ctx, &project_dir).await;
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project_id);

    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    wait_for_dsh_status(&ctx, &proj_url, "running").await;

    // The mock DSH writes its own PID into an isolated, per-project file.
    let pid_before = fs::read_to_string(mock_dsh_pid_file(&project_dir))
        .unwrap()
        .trim()
        .to_string();
    assert!(!pid_before.is_empty());
    let token_before = wait_for_dsh_token(&project_dir).await;
    assert!(!token_before.is_empty());

    let restart_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/restart",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(restart_res.status(), StatusCode::OK);
    wait_for_dsh_status(&ctx, &proj_url, "running").await;

    let pid_after = fs::read_to_string(mock_dsh_pid_file(&project_dir))
        .unwrap()
        .trim()
        .to_string();
    assert_ne!(
        pid_before, pid_after,
        "restart must run a new DSH process, not reuse the old one"
    );
    let token_after = wait_for_dsh_token(&project_dir).await;
    assert_ne!(
        token_before, token_after,
        "restart must replace the token, not reuse the old one"
    );
    assert_eq!(
        fs::read_to_string(dsh_token_path(&ctx.config.log_dir, project_id))
            .unwrap()
            .trim(),
        token_after,
        "restart must persist the new token in the Project runtime directory"
    );
    assert_eq!(mock_dsh_start_count(&project_dir), 2);

    let stop_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/stop",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stop_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dsh_link_is_omitted_when_token_is_missing() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("dsh-missing-token-proj");
    fs::create_dir_all(&project_dir).unwrap();
    let project_id = register(&ctx, &project_dir).await;
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project_id);

    let launch_url = format!(
        "http://{}/api/projects/{}/dsh/launch",
        ctx.server_addr, project_id
    );
    assert_eq!(
        ctx.client.post(&launch_url).send().await.unwrap().status(),
        StatusCode::OK
    );
    wait_for_dsh_status(&ctx, &proj_url, "running").await;
    let expected_token = wait_for_dsh_token(&project_dir).await;
    let token_path = dsh_token_path(&ctx.config.log_dir, project_id);

    // A blank host token copy must omit links even when the guest has emitted its token.
    fs::write(&token_path, "   \n").unwrap();
    let data: Value = ctx
        .client
        .get(&proj_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "running");
    assert!(
        data["links"]["local_dsh_url"].is_null(),
        "a blank token file must omit the local link"
    );
    assert!(
        data["links"]["tailnet_dsh_url"].is_null(),
        "a blank token file must omit the tailnet link"
    );
    assert!(
        data["links"]["dsh_url"].is_null(),
        "a blank token file must omit the compatibility link"
    );

    // Remove token file to simulate interval before token is emitted or a missing token.
    fs::remove_file(&token_path).unwrap();
    let _ = fs::remove_file(mock_dsh_token_file(&project_dir));
    wait_for_dsh_status(&ctx, &proj_url, "starting").await;

    // Readiness comes from guest state, including after the daemon loses its memory.
    let restarted_addr = spawn_daemon(&ctx.config).await;
    let restarted: Value = ctx
        .client
        .get(format!("http://{restarted_addr}/api/projects/{project_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(restarted["dsh_status"], "starting");
    assert!(restarted["links"]["dsh_url"].is_null());

    let data: Value = ctx
        .client
        .get(&proj_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "starting");
    assert!(
        data["links"]["local_dsh_url"].is_null(),
        "local_dsh_url must be omitted when token is missing"
    );
    assert!(
        data["links"]["tailnet_dsh_url"].is_null(),
        "tailnet_dsh_url must be omitted when token is missing"
    );
    assert!(
        data["links"]["dsh_url"].is_null(),
        "dsh_url must be omitted when token is missing"
    );

    // Capturing the URL token promotes the same live process to running.
    fs::write(mock_dsh_token_file(&project_dir), &expected_token).unwrap();
    fs::write(&token_path, &expected_token).unwrap();
    wait_for_dsh_status(&ctx, &proj_url, "running").await;
    let ready = wait_for_dsh_link(&ctx, &proj_url).await;
    assert_eq!(ready["dsh_status"], "running");

    // A stopped runtime must never present a stale token, whether or not the token file
    // lingered: the stop removes daemon token state with the guest process.
    let stop_res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/stop",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stop_res.status(), StatusCode::OK);
    assert!(
        !dsh_token_path(&ctx.config.log_dir, project_id).exists(),
        "stopping DSH must remove the daemon's token copy right away"
    );

    // Simulate stale daemon state surviving a guest-side DSH crash: the token file is
    // written back on the host, and the next status probe must drop it.
    fs::write(
        dsh_token_path(&ctx.config.log_dir, project_id),
        format!("{expected_token}\n"),
    )
    .unwrap();

    let data: Value = ctx
        .client
        .get(&proj_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "stopped");
    assert!(
        data["links"]["local_dsh_url"].is_null(),
        "a stopped runtime must never present an authenticated link"
    );
    assert!(
        data["links"]["tailnet_dsh_url"].is_null(),
        "a stopped runtime must never present an authenticated link"
    );
    assert!(
        data["links"]["dsh_url"].is_null(),
        "a stopped runtime must never present an authenticated link"
    );
    assert!(
        !dsh_token_path(&ctx.config.log_dir, project_id).exists(),
        "a fresh probe of a stopped runtime must remove the stale token copy"
    );
}

#[tokio::test]
async fn test_disconnect_during_vm_boot_does_not_leak_dsh_operation() {
    use tokio::io::AsyncWriteExt;
    let ctx = setup_test_server().await;
    let project_dir = ctx.home_dir.join("cancelled-launch");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".vm_start_slow"), "1").unwrap();
    let project_id = register(&ctx, &project_dir).await;
    let mut socket = tokio::net::TcpStream::connect(ctx.server_addr).await.unwrap();
    socket.write_all(format!(
        "POST /api/projects/{project_id}/dsh/launch HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n"
    ).as_bytes()).await.unwrap();
    let log = ctx.config.log_dir.join(project_id.to_string()).join("daemon.log");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if fs::read_to_string(&log).unwrap_or_default().contains("Invoking `devvm start`") { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    drop(socket);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let view: Value = ctx.client.get(format!("http://{}/api/projects/{project_id}", ctx.server_addr))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(view["dsh_status"], "stopped", "a disconnected launch must release its operation");
    assert!(!project_dir.join(".vm_running").exists(), "cancelled VM start must not keep executing");
    assert!(fs::read_to_string(log).unwrap().contains("cancelled"));
}

#[tokio::test]
async fn test_two_clients_coordinate_vm_start_and_stop() {
    let ctx = setup_test_server().await;
    let project_dir = ctx.home_dir.join("two-clients");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".vm_start_slow"), "1").unwrap();
    let project_id = register(&ctx, &project_dir).await;
    let base = format!("http://{}/api/projects/{project_id}", ctx.server_addr);
    let other = reqwest::Client::new();
    for action in ["vm/start", "dsh/launch"] {
        let log = ctx.config.log_dir.join(project_id.to_string()).join("daemon.log");
        let before = fs::read_to_string(&log).unwrap_or_default().matches("Invoking `devvm start`").count();
        let (client, url) = (ctx.client.clone(), format!("{base}/{action}"));
        let start = tokio::spawn(async move { client.post(url).send().await.unwrap() });
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if fs::read_to_string(&log).unwrap_or_default().matches("Invoking `devvm start`").count() > before { break; }
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        // UI B reads observed status, not UI A's pending action.
        let view: Value = other.get(&base).send().await.unwrap().json().await.unwrap();
        assert_eq!(view["vm_status"], "stopped");
        assert_eq!(view["dsh_status"], "stopped");
        for duplicate in ["vm/start", "dsh/launch", "dsh/restart"] {
            assert_eq!(other.post(format!("{base}/{duplicate}")).send().await.unwrap().status(), StatusCode::CONFLICT);
        }
        assert_eq!(other.post(format!("{base}/vm/stop")).send().await.unwrap().status(), StatusCode::OK);
        assert_eq!(start.await.unwrap().status(), StatusCode::CONFLICT);
        assert!(!project_dir.join(".vm_running").exists());
        assert_eq!(mock_dsh_start_count(&project_dir), 0);
        // Re-registering preserves identity but cannot preserve an abandoned lock.
        assert_eq!(other.post(format!("{base}/unregister")).send().await.unwrap().status(), StatusCode::OK);
        assert_eq!(register(&ctx, &project_dir).await, project_id);
    }
    fs::remove_file(project_dir.join(".vm_start_slow")).unwrap();
    assert_eq!(other.post(format!("{base}/vm/start")).send().await.unwrap().status(), StatusCode::OK);
    assert_eq!(other.post(format!("{base}/vm/stop")).send().await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dsh_start_failure_reports_the_command_stderr() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("dsh-start-fail-proj");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".dsh_start_fail"), "1").unwrap();
    let project_id = register(&ctx, &project_dir).await;

    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error: Value = res.json().await.unwrap();
    let message = error["error"].as_str().unwrap();
    assert!(
        message.contains("Mock DevVM: dsh could not be started"),
        "the HTTP body must carry the command stderr: {message}"
    );

    let daemon_log = fs::read_to_string(
        ctx.config
            .log_dir
            .join(project_id.to_string())
            .join("daemon.log"),
    )
    .unwrap();
    assert!(
        daemon_log.contains("Mock DevVM: dsh could not be started"),
        "daemon.log must carry the same text as the HTTP body: {daemon_log}"
    );

    // A failed start leaves no in-flight operation behind: the Project reads back as stopped.
    let data: Value = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, project_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "stopped");
    assert_eq!(mock_dsh_start_count(&project_dir), 0);
}

#[tokio::test]
async fn test_open_port_endpoint() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("open-port-proj");
    fs::create_dir_all(&project_dir).unwrap();

    let reg_url = format!("http://{}/api/projects/register", ctx.server_addr);
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    let p: Value = res.json().await.unwrap();
    let project_id = p["id"].as_str().unwrap();
    let project_host = p["project_host"].as_str().unwrap();

    let open_port_url = format!(
        "http://{}/api/projects/{}/open-port",
        ctx.server_addr, project_id
    );

    // 1. Open port 3000 -> returns local_url and tailnet_url
    let res = ctx
        .client
        .post(&open_port_url)
        .json(&json!({ "port": 3000 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let data: Value = res.json().await.unwrap();
    assert_eq!(
        data["local_url"],
        format!("http://3000.{}.devvm.localhost:8102", project_host)
    );
    assert_eq!(
        data["tailnet_url"],
        format!("https://{}-3000.risak.dev", project_host)
    );

    // 2. Open port with port 0 -> 400 Bad Request
    let res = ctx
        .client
        .post(&open_port_url)
        .json(&json!({ "port": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 3. Non-existent project id -> 404 Not Found
    let fake_id = Uuid::new_v4();
    let fake_open_port_url = format!(
        "http://{}/api/projects/{}/open-port",
        ctx.server_addr, fake_id
    );
    let res = ctx
        .client
        .post(&fake_open_port_url)
        .json(&json!({ "port": 3000 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_registration_validation_and_custom_uuid() {
    let ctx = setup_test_server().await;
    let reg_url = format!("http://{}/api/projects/register", ctx.server_addr);

    // 1. Existing valid custom UUID
    let custom_uuid = Uuid::new_v4();
    let proj_dir = ctx.home_dir.join("custom-uuid-proj");
    fs::create_dir_all(&proj_dir).unwrap();
    fs::write(proj_dir.join(".devvm-id"), format!("{}\n", custom_uuid)).unwrap();

    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": proj_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["id"].as_str().unwrap(), custom_uuid.to_string());

    // 2. Non-existent path
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": ctx.home_dir.join("does_not_exist").to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 3. File instead of directory
    let file_path = ctx.home_dir.join("some_file.txt");
    fs::write(&file_path, "not a dir").unwrap();
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": file_path.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_non_existent_project_operations() {
    let ctx = setup_test_server().await;
    let random_id = Uuid::new_v4();

    // 1. Get project
    let res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 2. Unregister
    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/unregister",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 3. VM start
    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/start",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 4. VM stop
    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/stop",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 5. VM delete
    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/vm/delete",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 6. DSH launch
    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 7. Logs for non-existent project returns empty logs with 200
    let res = ctx
        .client
        .get(format!(
            "http://{}/api/projects/{}/logs",
            ctx.server_addr, random_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let data: Value = res.json().await.unwrap();
    assert!(data["entries"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_browser_error_cases() {
    let ctx = setup_test_server().await;
    let base_url = format!("http://{}/api/browser", ctx.server_addr);

    // 1. Non-existent path
    let res = ctx
        .client
        .get(format!(
            "{}?path={}",
            base_url,
            ctx.home_dir.join("non_existent").to_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 2. File instead of directory
    let file_path = ctx.home_dir.join("test_file.txt");
    fs::write(&file_path, "hello").unwrap();
    let res = ctx
        .client
        .get(format!("{}?path={}", base_url, file_path.to_str().unwrap()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_no_generic_command_execution_endpoint() {
    let ctx = setup_test_server().await;
    let random_id = Uuid::new_v4();

    // Verify /api/exec or /api/projects/:id/exec is NOT exposed (404/405)
    let res = ctx
        .client
        .post(format!("http://{}/api/exec", ctx.server_addr))
        .json(&json!({ "command": "echo test" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let res = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/exec",
            ctx.server_addr, random_id
        ))
        .json(&json!({ "command": "echo test" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_ingress_logs_captured_and_workspace_clean() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("ingress-log-test-proj");
    fs::create_dir_all(&project_dir).unwrap();

    // Register project
    let reg_url = format!("http://{}/api/projects/register", ctx.server_addr);
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let project_data: Value = res.json().await.unwrap();
    let project_id = project_data["id"].as_str().unwrap();

    // Start VM
    let start_url = format!(
        "http://{}/api/projects/{}/vm/start",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&start_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify workspace is NOT polluted by .devvm-ingress.log
    assert!(
        !project_dir.join(".devvm-ingress.log").exists(),
        "Project workspace must not contain .devvm-ingress.log"
    );

    // Stop and delete VM
    let stop_url = format!(
        "http://{}/api/projects/{}/vm/stop",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&stop_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let del_url = format!(
        "http://{}/api/projects/{}/vm/delete",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&del_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Host-persisted logs must still exist and be readable via API
    let logs_url = format!(
        "http://{}/api/projects/{}/logs",
        ctx.server_addr, project_id
    );
    let res = ctx.client.get(&logs_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let logs_data: Value = res.json().await.unwrap();
    let logs = log_entries_text(&logs_data);

    assert!(logs.contains("Invoking `devvm start`"));
    assert!(logs.contains("Mock DevVM: started"));
    assert!(logs.contains("Invoking `devvm stop`"));
    assert!(logs.contains("Invoking `devvm rm`"));
}

#[tokio::test]
async fn test_ongoing_ingress_log_capture_persists_after_vm_deletion() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("ongoing-ingress-test-proj");
    fs::create_dir_all(&project_dir).unwrap();

    // 1. Register project
    let reg_url = format!("http://{}/api/projects/register", ctx.server_addr);
    let res = ctx
        .client
        .post(&reg_url)
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let project_data: Value = res.json().await.unwrap();
    let project_id_str = project_data["id"].as_str().unwrap();
    let project_id: Uuid = project_id_str.parse().unwrap();

    // 2. Start VM
    let start_url = format!(
        "http://{}/api/projects/{}/vm/start",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&start_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Simulate ongoing Caddy and FRP log output emitted into host-persisted path
    let ingress_log_dir = ctx
        ._temp_dir
        .path()
        .join("logs")
        .join(project_id.to_string());
    fs::create_dir_all(&ingress_log_dir).unwrap();
    let ingress_log_file = ingress_log_dir.join("ingress.log");
    fs::write(
        &ingress_log_file,
        "\u{1b}[1;34m[INFO] [client] [3080.ongoing-proj] start proxy success\u{1b}[0m\n\
         [INFO] [caddy] reverse_proxy: 127.0.0.1:3080 -> loopback upstream connected\n\
         [WARN] [client] heartbeat timeout, reconnecting to frps\n\
         \u{1b}[0m\n\
         incomplete trailing write",
    )
    .unwrap();

    // 4. Delete VM
    let del_url = format!(
        "http://{}/api/projects/{}/vm/delete",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&del_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 5. Verify no workspace pollution
    assert!(
        !project_dir.join(".devvm-ingress.log").exists(),
        "Project workspace must not contain .devvm-ingress.log"
    );

    // 6. Verify GET /api/projects/{id}/logs returns ongoing Caddy/FRP logs after deletion
    let logs_url = format!(
        "http://{}/api/projects/{}/logs",
        ctx.server_addr, project_id
    );
    let res = ctx.client.get(&logs_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let logs_data: Value = res.json().await.unwrap();
    let logs = log_entries_text(&logs_data);

    assert!(logs.contains("start proxy success"));
    assert!(logs.contains("reverse_proxy: 127.0.0.1:3080 -> loopback upstream connected"));
    assert!(logs.contains("heartbeat timeout, reconnecting to frps"));
    assert!(logs.contains("[ingress]"));
    assert!(
        !logs.contains('\u{1b}'),
        "terminal control sequences must be removed"
    );
    assert!(
        !logs.contains("incomplete trailing write"),
        "partial ingress writes must remain hidden until their newline arrives"
    );
    assert!(
        !logs.lines().any(|line| line.trim() == "[ingress]"),
        "reset-only terminal lines must not render"
    );
}

#[tokio::test]
async fn test_local_only_mode_omits_remote_links() {
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let devvm_bin = temp_dir.path().join("mock_devvm");
    create_mock_devvm(&devvm_bin, &log_dir);

    // Explicitly local-only configuration: remote_domain is None
    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path: config_dir.join("projects.json"),
        sync_config_path: config_dir.join("sync.json"),
        log_dir: log_dir.clone(),
        home_dir: home_dir.clone(),
        devvm_bin: devvm_bin.clone(),
        ingress_port: 8102,
        remote_domain: None,
    };

    let server_addr = spawn_daemon(&config).await;
    let client = reqwest::Client::new();

    let project_dir = home_dir.join("local-only-proj");
    fs::create_dir_all(&project_dir).unwrap();

    let reg_res: Value = client
        .post(format!("http://{}/api/projects/register", server_addr))
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let project_id = reg_res["id"].as_str().unwrap();

    // 1. Projects listing: remote templates and URLs must be omitted/null
    let list_res: Value = client
        .get(format!("http://{}/api/projects", server_addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let proj = &list_res[0];
    assert!(
        proj["links"]["tailnet_dsh_url"].is_null(),
        "tailnet_dsh_url must be null in local-only mode"
    );
    assert!(
        proj["links"]["tailnet_port_template"].is_null(),
        "tailnet_port_template must be null in local-only mode"
    );
    assert!(
        proj["links"]["local_port_template"]
            .as_str()
            .unwrap()
            .contains(".devvm.localhost:8102"),
        "local_port_template must be preserved"
    );

    // 2. Open port: remote tailnet_url must be omitted/null
    let port_res: Value = client
        .post(format!(
            "http://{}/api/projects/{}/open-port",
            server_addr, project_id
        ))
        .json(&json!({ "port": 3000 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        port_res["tailnet_url"].is_null(),
        "tailnet_url must be null in local-only mode"
    );
    assert!(
        port_res["local_url"].as_str().unwrap().contains("3000.")
            && port_res["local_url"]
                .as_str()
                .unwrap()
                .contains(".devvm.localhost:8102"),
        "local_url must be preserved"
    );

    // 3. Launch DSH: local_dsh_url gets populated with token, but tailnet_dsh_url remains null
    let launch_res = client
        .post(format!(
            "http://{}/api/projects/{}/dsh/launch",
            server_addr, project_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(launch_res.status(), StatusCode::OK);

    let proj_url = format!("http://{}/api/projects/{}", server_addr, project_id);
    let mut saw_running = false;
    for _ in 0..100 {
        let p: Value = client
            .get(&proj_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if p["dsh_status"] == "running" && !p["links"]["local_dsh_url"].is_null() {
            assert!(
                p["links"]["tailnet_dsh_url"].is_null(),
                "running DSH must omit tailnet link in local-only mode"
            );
            assert!(p["links"]["local_dsh_url"]
                .as_str()
                .unwrap()
                .contains("?token="));
            saw_running = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(saw_running, "DSH never reached running with local link");
    assert_eq!(client.post(format!("{proj_url}/dsh/stop")).send().await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn test_remote_url_dns_label_limit_handling() {
    let ctx = setup_test_server().await;

    // Create project with a very long directory name (over 70 characters)
    let long_name = "extremely-long-project-directory-name-that-exceeds-dns-label-limit-by-a-lot";
    let project_dir = ctx.home_dir.join(long_name);
    fs::create_dir_all(&project_dir).unwrap();

    let reg_res: Value = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({ "path": project_dir.to_str().unwrap() }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let project_id = reg_res["id"].as_str().unwrap();

    // Verify port template label is within 63 chars when rendered with a 5-digit port
    let list_res: Value = ctx
        .client
        .get(format!("http://{}/api/projects", ctx.server_addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let proj = list_res
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == project_id)
        .unwrap();
    let tailnet_tmpl = proj["links"]["tailnet_port_template"].as_str().unwrap();
    assert!(tailnet_tmpl.starts_with("https://"));
    assert!(tailnet_tmpl.ends_with(".risak.dev"));

    let rendered_tmpl = tailnet_tmpl.replace("{port}", "65535");
    let tmpl_host_part = rendered_tmpl
        .strip_prefix("https://")
        .unwrap()
        .strip_suffix(".risak.dev")
        .unwrap();
    assert!(
        tmpl_host_part.len() <= 63,
        "Template label length {} with 5-digit port exceeds 63 chars",
        tmpl_host_part.len()
    );
    let project_host = proj["project_host"].as_str().unwrap();
    assert_eq!(
        project_host.len(),
        8,
        "Long names use their existing hash only"
    );
    assert_eq!(tmpl_host_part, format!("{project_host}-65535"));
    assert!(tmpl_host_part.ends_with("-65535"));

    // Verify open port label is within 63 chars and valid
    let port_res: Value = ctx
        .client
        .post(format!(
            "http://{}/api/projects/{}/open-port",
            ctx.server_addr, project_id
        ))
        .json(&json!({ "port": 8080 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let remote_url = port_res["tailnet_url"].as_str().unwrap();
    assert!(remote_url.starts_with("https://"));
    let host_part = remote_url
        .strip_prefix("https://")
        .unwrap()
        .strip_suffix(".risak.dev")
        .unwrap();
    assert!(
        host_part.len() <= 63,
        "Host label length {} exceeds 63 chars",
        host_part.len()
    );
    assert!(host_part.ends_with("-8080"), "Must end with port");
}
