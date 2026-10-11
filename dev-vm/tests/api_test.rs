mod common;

use common::{
    devvm_status_output, exited, is_devvm, is_guest_exec, log_entries_text, ready, verify,
    MockRunner, DEVVM, DSH_LAUNCH, DSH_STATUS, DSH_STOP,
};
use devvm_daemon::{create_router, logs::dsh_token_path, AppState, CommandFuture, DaemonConfig};
use mockall::Sequence;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tempfile::tempdir;
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle};
use uuid::Uuid;

struct TestContext {
    _temp_dir: tempfile::TempDir,
    home_dir: PathBuf,
    config: DaemonConfig,
    server_addr: SocketAddr,
    client: reqwest::Client,
    runner: Arc<MockRunner>,
    servers: Vec<JoinHandle<()>>,
}

impl TestContext {
    async fn restart(&mut self) -> SocketAddr {
        let (addr, server) = spawn_daemon(&self.config, self.runner.clone()).await;
        self.servers.push(server);
        addr
    }

    async fn finish(self) {
        for server in self.servers {
            server.abort();
            let _ = server.await;
        }
        verify(self.runner);
    }
}

async fn spawn_daemon(
    config: &DaemonConfig,
    runner: Arc<MockRunner>,
) -> (SocketAddr, JoinHandle<()>) {
    let router = create_router(AppState::new(config.clone(), runner));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (addr, server)
}

async fn setup_test_server(
    expect: impl FnOnce(&Path, &mut MockRunner, &mut Sequence),
) -> TestContext {
    setup_server(Some("risak.dev".into()), expect).await
}

async fn setup_server(
    remote_domain: Option<String>,
    expect: impl FnOnce(&Path, &mut MockRunner, &mut Sequence),
) -> TestContext {
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();
    let config = DaemonConfig {
        host: "127.0.0.1".into(),
        port: 0,
        config_path: temp_dir.path().join("projects.json"),
        sync_config_path: temp_dir.path().join("sync.json"),
        log_dir: temp_dir.path().join("logs"),
        home_dir: home_dir.clone(),
        devvm_bin: PathBuf::from(DEVVM),
        ingress_port: 8102,
        remote_domain,
    };
    let mut runner = MockRunner::new();
    expect(&home_dir, &mut runner, &mut Sequence::new());
    let runner = Arc::new(runner);
    let (server_addr, server) = spawn_daemon(&config, runner.clone()).await;
    TestContext {
        _temp_dir: temp_dir,
        home_dir,
        config,
        server_addr,
        client: reqwest::Client::new(),
        runner,
        servers: vec![server],
    }
}

fn command(
    runner: &mut MockRunner,
    seq: &mut Sequence,
    dir: &Path,
    args: &'static [&'static str],
    code: i32,
    stdout: &str,
    stderr: &str,
) {
    let (dir, stdout, stderr) = (dir.to_owned(), stdout.to_owned(), stderr.to_owned());
    runner
        .expect_run()
        .withf(move |c, _| is_devvm(c, &dir, args))
        .times(1)
        .in_sequence(seq)
        .returning(move |_, _| ready(exited(code, &stdout, &stderr)));
}

fn status(runner: &mut MockRunner, seq: &mut Sequence, dir: &Path, state: &str) {
    let dir = dir.to_owned();
    let state = state.to_owned();
    runner
        .expect_run()
        .withf(move |c, _| is_devvm(c, &dir, &["status"]))
        .times(1)
        .in_sequence(seq)
        .returning(move |_, _| ready(devvm_status_output(&state)));
}

fn guest(
    runner: &mut MockRunner,
    seq: &mut Sequence,
    dir: &Path,
    marker: &'static str,
    code: i32,
    stderr: &str,
) {
    let (dir, stderr) = (dir.to_owned(), stderr.to_owned());
    runner
        .expect_run()
        .withf(move |c, _| is_guest_exec(c, &dir, marker))
        .times(1)
        .in_sequence(seq)
        .returning(move |_, _| ready(exited(code, "", &stderr)));
}

fn view(runner: &mut MockRunner, seq: &mut Sequence, dir: &Path, dsh_code: i32) {
    status(runner, seq, dir, "running");
    guest(runner, seq, dir, DSH_STATUS, dsh_code, "");
    command(
        runner,
        seq,
        dir,
        &["memory"],
        0,
        r#"{"host_bytes":1610612736,"guest_bytes":1073741824,"limit_bytes":8589934592}"#,
        "",
    );
}

fn held_start(
    runner: &mut MockRunner,
    seq: &mut Sequence,
    dir: &Path,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    cancelled: Arc<Notify>,
) {
    let dir = dir.to_owned();
    runner
        .expect_run()
        .withf(move |c, _| is_devvm(c, &dir, &["start"]))
        .times(1)
        .in_sequence(seq)
        .returning(move |_, cancel| {
            let (entered, release, cancelled) = (entered.clone(), release.clone(), cancelled.clone());
            Box::pin(async move {
                entered.notify_one();
                tokio::select! {
                    _ = cancel.cancelled() => {
                        cancelled.notify_one();
                        Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "command cancelled"))
                    }
                    _ = release.notified() => exited(0, "", ""),
                }
            }) as CommandFuture
        });
}

async fn register(ctx: &TestContext, project_dir: &Path) -> Uuid {
    let project: Value = ctx
        .client
        .post(format!("http://{}/api/projects/register", ctx.server_addr))
        .json(&json!({"path": project_dir}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    project["id"].as_str().unwrap().parse().unwrap()
}

async fn project_view(ctx: &TestContext, id: Uuid) -> Value {
    ctx.client
        .get(format!("http://{}/api/projects/{id}", ctx.server_addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn write_token(ctx: &TestContext, id: Uuid, token: &str) {
    let path = dsh_token_path(&ctx.config.log_dir, id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, token).unwrap();
}

async fn post(ctx: &TestContext, id: Uuid, action: &str) -> reqwest::Response {
    ctx.client
        .post(format!(
            "http://{}/api/projects/{id}/{action}",
            ctx.server_addr
        ))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_embedded_ui_served() {
    let ctx = setup_test_server(|_, _, _| {}).await;
    let url = format!("http://{}", ctx.server_addr);
    let res = ctx.client.get(&url).send().await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.text().await.unwrap();
    assert!(body.contains("<!doctype html>"));
    assert!(body.contains("Your workspace"));
    assert!(body.contains("/manifest.webmanifest"));
    assert!(body.contains("/app/app.js"));
    let res = ctx
        .client
        .get(format!("{url}/manage"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.text().await.unwrap();
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_control_app_names_follow_subdomain() {
    let ctx = setup_test_server(|_, _, _| {}).await;
    let base = format!("http://{}", ctx.server_addr);
    for (host, name) in [
        ("devvm.risak.dev", "devvm"),
        ("devvm-risak.risak.dev", "devvm-risak"),
        ("devvm-3080.risak.dev:443", "devvm-3080"),
        ("control.devvm.localhost:8100", "DevVM"),
        ("devvm2.risak.dev", "DevVM"),
        ("devvm-.risak.dev", "DevVM"),
        ("devvm-risak.other.dev", "DevVM"),
        ("devvm-risak.nested.risak.dev", "DevVM"),
    ] {
        let response = ctx
            .client
            .get(format!("{base}/manifest.webmanifest"))
            .header("Host", host)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{host}");
        assert_eq!(
            response.headers()["content-type"],
            "application/manifest+json"
        );
        let manifest: Value = response.json().await.unwrap();
        assert_eq!(manifest["name"], name, "{host}");
        assert_eq!(manifest["short_name"], name, "{host}");
        assert_eq!(manifest["id"], "/");
        assert_eq!(manifest["start_url"], "/");
        assert_eq!(manifest["scope"], "/");
        let html = ctx
            .client
            .get(&base)
            .header("Host", host)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(html.contains(&format!("<title>{name}</title>")), "{host}");
        assert!(
            html.contains(&format!(
                "name=\"apple-mobile-web-app-title\" content=\"{name}\""
            )),
            "{host}"
        );
    }
    ctx.finish().await;
}

#[tokio::test]
async fn test_project_logs_never_return_the_dsh_token_file() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("token-log-proj");
        for _ in 0..1 {
            status(runner, seq, &dir, "stopped");
        }
    })
    .await;
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_project_logs_merge_three_sources_in_time_order() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("merged-logs-proj");
        for _ in 0..1 {
            status(runner, seq, &dir, "stopped");
        }
    })
    .await;
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_project_browser_jail() {
    let ctx = setup_test_server(|_, _, _| {}).await;

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
    ctx.finish().await;
}

#[tokio::test]
async fn test_project_registration_and_id_lifecycle() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("awesome-app");
        for _ in 0..3 {
            status(runner, seq, &dir, "stopped");
        }
    })
    .await;

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
    ctx.finish().await;
}

#[tokio::test]
async fn test_vm_lifecycle_operations() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("vm-test-proj");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "Machine started\n", "");
        view(runner, seq, &dir, 1);
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        command(runner, seq, &dir, &["stop"], 0, "", "");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["rm"], 0, "", "");
    })
    .await;

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
    assert!(data["memory"].is_null());

    // 2. Start VM
    let start_url = format!(
        "http://{}/api/projects/{}/vm/start",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&start_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify VM is now running with memory metrics
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["vm_status"], "running");
    assert_eq!(data["memory"]["formatted"], "1.5 GiB (1 GiB) / 8 GiB");
    assert_eq!(data["memory"]["host_bytes"], 1610612736);
    assert_eq!(data["memory"]["guest_bytes"], 1073741824);
    assert_eq!(data["memory"]["limit_bytes"], 8589934592_u64);

    // 3. Stop VM
    let stop_url = format!(
        "http://{}/api/projects/{}/vm/stop",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&stop_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify VM is now stopped and memory is null
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["vm_status"], "stopped");
    assert!(data["memory"].is_null());

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
    assert!(logs.contains("`devvm start` succeeded"));
    assert!(logs.contains("Invoking `devvm stop`"));
    assert!(logs.contains("Invoking `devvm rm`"));
    ctx.finish().await;
}

#[tokio::test]
async fn test_dsh_status_is_read_from_the_devvm_and_survives_a_daemon_restart() {
    let mut ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("dsh-test-proj");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "", "");
        guest(runner, seq, &dir, DSH_LAUNCH, 0, "");
        view(runner, seq, &dir, 0);
        view(runner, seq, &dir, 0);
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        view(runner, seq, &dir, 1);
    })
    .await;
    let dir = ctx.home_dir.join("dsh-test-proj");
    fs::create_dir_all(&dir).unwrap();
    let id = register(&ctx, &dir).await;
    assert_eq!(post(&ctx, id, "dsh/launch").await.status(), StatusCode::OK);
    write_token(&ctx, id, "test-launch-token");
    fs::write(
        ctx.config.log_dir.join(id.to_string()).join("dsh.log"),
        "[2026-03-03T01:15:17.000Z] dsh web: http://127.0.0.1:3080/?token=[redacted]\n",
    )
    .unwrap();
    let data = project_view(&ctx, id).await;
    assert_eq!(data["vm_status"], "running");
    assert_eq!(data["dsh_status"], "running");
    let host = data["project_host"].as_str().unwrap();
    let local = format!("http://3080.{host}.devvm.localhost:8102?token=test-launch-token");
    let remote = format!("https://{host}-3080.risak.dev?token=test-launch-token");
    assert_eq!(data["links"]["local_dsh_url"], local);
    assert_eq!(data["links"]["tailnet_dsh_url"], remote);
    assert_eq!(data["links"]["dsh_url"], local);
    let restarted = ctx.restart().await;
    let after: Value = ctx
        .client
        .get(format!("http://{restarted}/api/projects/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["dsh_status"], "running");
    assert_eq!(after["links"], data["links"]);
    assert_ne!(after["daemon_instance_id"], data["daemon_instance_id"]);
    let logs: Value = ctx
        .client
        .get(format!("http://{}/api/projects/{id}/logs", ctx.server_addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(logs["entries"].as_array().unwrap().iter().any(|entry| {
        entry["source"] == "dsh"
            && entry["ts"] == "2026-03-03T01:15:17.000Z"
            && entry["message"].as_str().unwrap().contains("dsh web:")
    }));
    assert_eq!(post(&ctx, id, "dsh/stop").await.status(), StatusCode::OK);
    assert!(!dsh_token_path(&ctx.config.log_dir, id).exists());
    let stopped = project_view(&ctx, id).await;
    assert_eq!(stopped["dsh_status"], "stopped");
    assert!(stopped["links"]["dsh_url"].is_null());
    ctx.finish().await;
}

#[tokio::test]
async fn test_second_launch_preserves_the_running_dsh_token() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("dsh-idempotent-proj");
        status(runner, seq, &dir, "stopped");
        for _ in 0..2 {
            status(runner, seq, &dir, "running");
            guest(runner, seq, &dir, DSH_LAUNCH, 0, "");
            view(runner, seq, &dir, 0);
        }
    })
    .await;
    let dir = ctx.home_dir.join("dsh-idempotent-proj");
    fs::create_dir_all(&dir).unwrap();
    let id = register(&ctx, &dir).await;
    write_token(&ctx, id, "current-token");
    for _ in 0..2 {
        assert_eq!(post(&ctx, id, "dsh/launch").await.status(), StatusCode::OK);
        let data = project_view(&ctx, id).await;
        assert_eq!(data["dsh_status"], "running");
        assert!(data["links"]["dsh_url"]
            .as_str()
            .unwrap()
            .ends_with("?token=current-token"));
        assert_eq!(
            fs::read_to_string(dsh_token_path(&ctx.config.log_dir, id)).unwrap(),
            "current-token"
        );
    }
    ctx.finish().await;
}

#[tokio::test]
async fn test_dsh_restart_stops_before_launching_and_refreshes_links() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("dsh-restart-proj");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_LAUNCH, 0, "");
        view(runner, seq, &dir, 0);
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_LAUNCH, 0, "");
        view(runner, seq, &dir, 0);
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
    })
    .await;
    let dir = ctx.home_dir.join("dsh-restart-proj");
    fs::create_dir_all(&dir).unwrap();
    let id = register(&ctx, &dir).await;
    assert_eq!(post(&ctx, id, "dsh/launch").await.status(), StatusCode::OK);
    write_token(&ctx, id, "first-token");
    assert!(project_view(&ctx, id).await["links"]["dsh_url"]
        .as_str()
        .unwrap()
        .ends_with("?token=first-token"));
    assert_eq!(post(&ctx, id, "dsh/restart").await.status(), StatusCode::OK);
    assert!(!dsh_token_path(&ctx.config.log_dir, id).exists());
    write_token(&ctx, id, "second-token");
    let data = project_view(&ctx, id).await;
    assert_eq!(data["dsh_status"], "running");
    assert!(data["links"]["dsh_url"]
        .as_str()
        .unwrap()
        .ends_with("?token=second-token"));
    assert_eq!(post(&ctx, id, "dsh/stop").await.status(), StatusCode::OK);
    ctx.finish().await;
}

#[tokio::test]
async fn test_dsh_link_is_omitted_when_token_is_missing() {
    let mut ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("dsh-missing-token-proj");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_LAUNCH, 0, "");
        view(runner, seq, &dir, 0);
        view(runner, seq, &dir, 2);
        view(runner, seq, &dir, 2);
        view(runner, seq, &dir, 0);
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        view(runner, seq, &dir, 1);
    })
    .await;
    let dir = ctx.home_dir.join("dsh-missing-token-proj");
    fs::create_dir_all(&dir).unwrap();
    let id = register(&ctx, &dir).await;
    assert_eq!(post(&ctx, id, "dsh/launch").await.status(), StatusCode::OK);
    write_token(&ctx, id, "   \n");
    let blank = project_view(&ctx, id).await;
    assert_eq!(blank["dsh_status"], "running");
    for key in ["local_dsh_url", "tailnet_dsh_url", "dsh_url"] {
        assert!(blank["links"][key].is_null());
    }
    fs::remove_file(dsh_token_path(&ctx.config.log_dir, id)).unwrap();
    let missing = project_view(&ctx, id).await;
    assert_eq!(missing["dsh_status"], "starting");
    for key in ["local_dsh_url", "tailnet_dsh_url", "dsh_url"] {
        assert!(missing["links"][key].is_null());
    }
    let restarted = ctx.restart().await;
    let after: Value = ctx
        .client
        .get(format!("http://{restarted}/api/projects/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["dsh_status"], "starting");
    for key in ["local_dsh_url", "tailnet_dsh_url", "dsh_url"] {
        assert!(after["links"][key].is_null());
    }
    write_token(&ctx, id, "ready-token");
    let ready = project_view(&ctx, id).await;
    assert_eq!(ready["dsh_status"], "running");
    assert!(ready["links"]["dsh_url"]
        .as_str()
        .unwrap()
        .ends_with("?token=ready-token"));
    assert_eq!(post(&ctx, id, "dsh/stop").await.status(), StatusCode::OK);
    assert!(!dsh_token_path(&ctx.config.log_dir, id).exists());
    write_token(&ctx, id, "stale-token");
    let stopped = project_view(&ctx, id).await;
    assert_eq!(stopped["dsh_status"], "stopped");
    for key in ["local_dsh_url", "tailnet_dsh_url", "dsh_url"] {
        assert!(stopped["links"][key].is_null());
    }
    assert!(!dsh_token_path(&ctx.config.log_dir, id).exists());
    ctx.finish().await;
}

#[tokio::test]
async fn test_disconnect_during_vm_boot_does_not_leak_dsh_operation() {
    use tokio::io::AsyncWriteExt;
    let entered = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("cancelled-launch");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        held_start(
            runner,
            seq,
            &dir,
            entered.clone(),
            Arc::new(Notify::new()),
            cancelled.clone(),
        );
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "", "");
    })
    .await;
    let dir = ctx.home_dir.join("cancelled-launch");
    fs::create_dir_all(&dir).unwrap();
    let id = register(&ctx, &dir).await;
    let mut socket = tokio::net::TcpStream::connect(ctx.server_addr)
        .await
        .unwrap();
    socket.write_all(format!("POST /api/projects/{id}/dsh/launch HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n").as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    drop(socket);
    tokio::time::timeout(Duration::from_secs(3), cancelled.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot: Value = ctx
                .client
                .get(format!(
                    "http://{}/api/projects/{id}/operations",
                    ctx.server_addr
                ))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if snapshot["active"].is_null() && !snapshot["recent"].as_array().unwrap().is_empty() {
                assert_eq!(snapshot["recent"][0]["state"], "cancelled");
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(project_view(&ctx, id).await["dsh_status"], "stopped");
    assert_eq!(post(&ctx, id, "vm/start").await.status(), StatusCode::OK);
    let log =
        fs::read_to_string(ctx.config.log_dir.join(id.to_string()).join("daemon.log")).unwrap();
    assert!(log.contains("cancelled"));
    ctx.finish().await;
}

#[tokio::test]
async fn test_two_clients_coordinate_vm_start_and_stop() {
    let entered = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("two-clients");
        status(runner, seq, &dir, "stopped");
        for action in ["vm/start", "dsh/launch"] {
            if action == "dsh/launch" {
                status(runner, seq, &dir, "stopped");
            }
            held_start(
                runner,
                seq,
                &dir,
                entered.clone(),
                Arc::new(Notify::new()),
                cancelled.clone(),
            );
            status(runner, seq, &dir, "stopped");
            status(runner, seq, &dir, "stopped");
            command(runner, seq, &dir, &["stop"], 0, "", "");
            status(runner, seq, &dir, "stopped");
        }
        command(runner, seq, &dir, &["start"], 0, "", "");
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        command(runner, seq, &dir, &["stop"], 0, "", "");
    })
    .await;
    let dir = ctx.home_dir.join("two-clients");
    fs::create_dir_all(&dir).unwrap();
    let id = register(&ctx, &dir).await;
    let base = format!("http://{}/api/projects/{id}", ctx.server_addr);
    let other = reqwest::Client::new();
    for action in ["vm/start", "dsh/launch"] {
        let (client, url) = (ctx.client.clone(), format!("{base}/{action}"));
        let start = tokio::spawn(async move { client.post(url).send().await.unwrap() });
        tokio::time::timeout(Duration::from_secs(3), entered.notified())
            .await
            .unwrap();
        let data: Value = other.get(&base).send().await.unwrap().json().await.unwrap();
        assert_eq!(data["vm_status"], "stopped");
        assert_eq!(data["dsh_status"], "stopped");
        assert!(!data["operation"].is_null());
        for duplicate in ["vm/start", "dsh/launch", "dsh/restart"] {
            assert_eq!(
                other
                    .post(format!("{base}/{duplicate}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
        }
        assert_eq!(
            other
                .post(format!("{base}/vm/stop"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        tokio::time::timeout(Duration::from_secs(3), cancelled.notified())
            .await
            .unwrap();
        assert_eq!(start.await.unwrap().status(), StatusCode::CONFLICT);
        assert_eq!(
            other
                .post(format!("{base}/unregister"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(register(&ctx, &dir).await, id);
    }
    assert_eq!(post(&ctx, id, "vm/start").await.status(), StatusCode::OK);
    assert_eq!(post(&ctx, id, "vm/stop").await.status(), StatusCode::OK);
    ctx.finish().await;
}

#[tokio::test]
async fn test_dsh_start_failure_reports_the_command_stderr() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("dsh-start-fail-proj");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "", "");
        guest(runner, seq, &dir, DSH_LAUNCH, 1, "dsh could not be started");
        view(runner, seq, &dir, 1);
    })
    .await;

    let project_dir = ctx.home_dir.join("dsh-start-fail-proj");
    fs::create_dir_all(&project_dir).unwrap();
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
        message.contains("dsh could not be started"),
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
        daemon_log.contains("dsh could not be started"),
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_open_port_endpoint() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("open-port-proj");
        for _ in 0..1 {
            status(runner, seq, &dir, "stopped");
        }
    })
    .await;

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
    ctx.finish().await;
}

#[tokio::test]
async fn test_registration_validation_and_custom_uuid() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("custom-uuid-proj");
        for _ in 0..1 {
            status(runner, seq, &dir, "stopped");
        }
    })
    .await;
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_non_existent_project_operations() {
    let ctx = setup_test_server(|_, _, _| {}).await;
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_browser_error_cases() {
    let ctx = setup_test_server(|_, _, _| {}).await;
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_no_generic_command_execution_endpoint() {
    let ctx = setup_test_server(|_, _, _| {}).await;
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_ingress_and_lifecycle_logs_survive_vm_deletion() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("ingress-log-test-proj");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "Machine started\n", "");
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        command(runner, seq, &dir, &["stop"], 0, "", "");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["rm"], 0, "", "");
    })
    .await;

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

    let ingress_log = ctx.config.log_dir.join(project_id).join("ingress.log");
    fs::write(
        &ingress_log,
        "[2026-03-03T01:15:17.000Z] ingress proxy connected\n",
    )
    .unwrap();

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

    assert!(logs.contains("[ingress] ingress proxy connected"));
    assert!(logs.contains("Invoking `devvm start`"));
    assert!(logs.contains("`devvm start` succeeded"));
    assert!(logs.contains("Invoking `devvm stop`"));
    assert!(logs.contains("Invoking `devvm rm`"));
    ctx.finish().await;
}

#[tokio::test]
async fn test_ongoing_ingress_log_capture_persists_after_vm_deletion() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("ongoing-ingress-test-proj");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "Machine started\n", "");
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
        command(runner, seq, &dir, &["rm"], 0, "", "");
    })
    .await;

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

    // 5. Read the persisted Caddy/FRP logs after deletion.
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
    ctx.finish().await;
}

#[tokio::test]
async fn test_local_only_mode_omits_remote_links() {
    let ctx = setup_server(None, |home, runner, seq| {
        let dir = home.join("local-only-proj");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "", "");
        guest(runner, seq, &dir, DSH_LAUNCH, 0, "");
        view(runner, seq, &dir, 0);
        status(runner, seq, &dir, "running");
        guest(runner, seq, &dir, DSH_STOP, 0, "");
    })
    .await;
    let (home_dir, server_addr, client) = (&ctx.home_dir, ctx.server_addr, &ctx.client);
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

    write_token(&ctx, project_id.parse().unwrap(), "local-token");
    let proj_url = format!("http://{}/api/projects/{}", server_addr, project_id);
    let data: Value = client
        .get(&proj_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["dsh_status"], "running");
    assert!(data["links"]["tailnet_dsh_url"].is_null());
    assert!(data["links"]["local_dsh_url"]
        .as_str()
        .unwrap()
        .ends_with("?token=local-token"));
    assert_eq!(
        client
            .post(format!("{proj_url}/dsh/stop"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    ctx.finish().await;
}

#[tokio::test]
async fn test_remote_url_dns_label_limit_handling() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home
            .join("extremely-long-project-directory-name-that-exceeds-dns-label-limit-by-a-lot");
        for _ in 0..2 {
            status(runner, seq, &dir, "stopped");
        }
    })
    .await;

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
    ctx.finish().await;
}

async fn wait_for_operation_result(ctx: &TestContext, project: Uuid, operation: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot: Value = ctx
                .client
                .get(format!(
                    "http://{}/api/projects/{project}/operations",
                    ctx.server_addr
                ))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if let Some(result) = snapshot["recent"]
                .as_array()
                .unwrap()
                .iter()
                .find(|record| record["id"] == operation)
            {
                return result.clone();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("accepted operation did not settle")
}

#[tokio::test]
async fn test_submitted_operation_survives_lost_http_acknowledgement() {
    use tokio::io::AsyncWriteExt;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let mut ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("submitted-start");
        status(runner, seq, &dir, "stopped");
        held_start(
            runner,
            seq,
            &dir,
            entered.clone(),
            release.clone(),
            cancelled.clone(),
        );
        view(runner, seq, &dir, 1);
        view(runner, seq, &dir, 1);
    })
    .await;
    let project_dir = ctx.home_dir.join("submitted-start");
    fs::create_dir_all(&project_dir).unwrap();
    let project = register(&ctx, &project_dir).await;
    let request_id = Uuid::new_v4();
    let body = json!({"action": "start_vm", "request_id": request_id}).to_string();
    let mut socket = tokio::net::TcpStream::connect(ctx.server_addr)
        .await
        .unwrap();
    socket.write_all(format!("POST /api/projects/{project}/operations HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let operation = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot: Value = ctx
                .client
                .get(format!(
                    "http://{}/api/projects/{project}/operations",
                    ctx.server_addr
                ))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if snapshot["active"]["request_id"] == request_id.to_string() {
                break snapshot["active"]["id"].as_str().unwrap().to_owned();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    drop(socket); // The client never consumes the accepted HTTP response.
    release.notify_one();
    let result = wait_for_operation_result(&ctx, project, &operation).await;
    assert_eq!(result["state"], "succeeded");
    assert_eq!(result["request_id"], request_id.to_string());
    let view: Value = ctx
        .client
        .get(format!("http://{}/api/projects/{project}", ctx.server_addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(view["vm_status"], "running");
    assert!(view["operation"].is_null());
    assert_eq!(view["last_operation"]["id"], operation);
    let restarted = ctx.restart().await;
    let after: Value = ctx
        .client
        .get(format!("http://{restarted}/api/projects/{project}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["vm_status"], "running");
    assert_ne!(after["daemon_instance_id"], view["daemon_instance_id"]);
    assert!(
        after["last_operation"].is_null(),
        "restart must not invent the lost operation result"
    );
    ctx.finish().await;
}

#[tokio::test]
async fn test_legacy_stop_preempts_submitted_start_and_preserves_both_results() {
    let entered = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("mixed-ownership");
        status(runner, seq, &dir, "stopped");
        held_start(
            runner,
            seq,
            &dir,
            entered.clone(),
            Arc::new(Notify::new()),
            cancelled.clone(),
        );
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["stop"], 0, "", "");
    })
    .await;
    let project_dir = ctx.home_dir.join("mixed-ownership");
    fs::create_dir_all(&project_dir).unwrap();
    let project = register(&ctx, &project_dir).await;
    let base = format!("http://{}/api/projects/{project}", ctx.server_addr);
    let accepted = ctx
        .client
        .post(format!("{base}/operations"))
        .json(&json!({"action": "start_vm", "request_id": Uuid::new_v4()}))
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let receipt: Value = accepted.json().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    assert_eq!(
        ctx.client
            .post(format!("{base}/vm/start"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        ctx.client
            .post(format!("{base}/vm/stop"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let result = wait_for_operation_result(&ctx, project, receipt["id"].as_str().unwrap()).await;
    assert_eq!(result["state"], "cancelled");
    let snapshot: Value = ctx
        .client
        .get(format!("{base}/operations"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(snapshot["active"].is_null());
    assert_eq!(snapshot["recent"][0]["action"], "VM stop");
    assert_eq!(snapshot["recent"][0]["state"], "succeeded");
    tokio::time::timeout(Duration::from_secs(3), cancelled.notified())
        .await
        .unwrap();
    ctx.finish().await;
}

#[tokio::test]
async fn test_submitted_failure_is_observable_and_logged_once_after_unregister() {
    let ctx = setup_test_server(|home, runner, seq| {
        let dir = home.join("submitted-failure");
        status(runner, seq, &dir, "stopped");
        status(runner, seq, &dir, "stopped");
        command(runner, seq, &dir, &["start"], 0, "", "");
        guest(runner, seq, &dir, DSH_LAUNCH, 1, "dsh could not be started");
    })
    .await;
    let project_dir = ctx.home_dir.join("submitted-failure");
    fs::create_dir_all(&project_dir).unwrap();
    let project = register(&ctx, &project_dir).await;
    let base = format!("http://{}/api/projects/{project}", ctx.server_addr);
    let accepted = ctx
        .client
        .post(format!("{base}/operations"))
        .json(&json!({"action": "launch_dsh", "request_id": Uuid::new_v4()}))
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let receipt: Value = accepted.json().await.unwrap();
    let failed = wait_for_operation_result(&ctx, project, receipt["id"].as_str().unwrap()).await;
    assert_eq!(failed["state"], "failed");
    let error = failed["error"].as_str().unwrap();
    let log = fs::read_to_string(
        ctx.config
            .log_dir
            .join(project.to_string())
            .join("daemon.log"),
    )
    .unwrap();
    assert_eq!(
        log.matches(error).count(),
        1,
        "async failure must be logged exactly once"
    );
    assert_eq!(
        ctx.client
            .post(format!("{base}/unregister"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let snapshot: Value = ctx
        .client
        .get(format!("{base}/operations"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(snapshot["recent"][0]["id"], receipt["id"]);
    assert_eq!(
        ctx.client
            .get(format!(
                "http://{}/api/projects/{}/operations",
                ctx.server_addr,
                Uuid::new_v4()
            ))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    ctx.finish().await;
}
