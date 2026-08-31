mod common;

use common::create_mock_devvm;
use devvm_daemon::{create_router, AppState, DaemonConfig, DshRuntimeManager, SyncManager};
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
    server_addr: SocketAddr,
    client: reqwest::Client,
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
    create_mock_devvm(&devvm_bin);

    let config = DaemonConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        config_path: config_path.clone(),
        sync_config_path: config_dir.join("sync.json"),
        log_dir: log_dir.clone(),
        home_dir: home_dir.clone(),
        devvm_bin: devvm_bin.clone(),
        ingress_port: 8102,
        tailnet_domain: "devvm.internal".to_string(),
    };

    let dsh_runtime_manager = DshRuntimeManager::new();
    let sync_manager = SyncManager::new();
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

    TestContext {
        _temp_dir: temp_dir,
        home_dir,
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
    assert!(body.contains("VM: ${vmStatus.label}"));
    assert!(body.contains("DSH: ${dshStatus.label}"));
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
            "http://{{port}}.{}.devvm.internal:8102",
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
    let logs = logs_data["logs"].as_str().unwrap();
    assert!(logs.contains("Invoking `devvm start`"));
    assert!(logs.contains("Mock DevVM: started"));
    assert!(logs.contains("Invoking `devvm stop`"));
    assert!(logs.contains("Invoking `devvm rm`"));
}

#[tokio::test]
async fn test_dsh_runtime_lifecycle_and_failure_detection() {
    let ctx = setup_test_server().await;

    let project_dir = ctx.home_dir.join("dsh-test-proj");
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
    let proj_url = format!("http://{}/api/projects/{}", ctx.server_addr, project_id);

    // 1. Launch DSH when VM is stopped -> should start VM and report starting until ready
    let launch_url = format!(
        "http://{}/api/projects/{}/dsh/launch",
        ctx.server_addr, project_id
    );
    fs::write(project_dir.join(".dsh_start_slow"), "1").unwrap();
    let launch_client = ctx.client.clone();
    let launch_url_clone = launch_url.clone();
    let launch =
        tokio::spawn(async move { launch_client.post(&launch_url_clone).send().await.unwrap() });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["dsh_status"], "starting");
    assert!(data["links"]["local_dsh_url"].is_null());

    let res = launch.await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    fs::remove_file(project_dir.join(".dsh_start_slow")).unwrap();

    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["vm_status"], "running");
    assert_eq!(data["dsh_status"], "running");
    assert_eq!(
        data["links"]["local_dsh_url"].as_str().unwrap(),
        format!("http://3080.{}.devvm.localhost:8102", project_host)
    );
    assert_eq!(
        data["links"]["tailnet_dsh_url"].as_str().unwrap(),
        format!("http://3080.{}.devvm.internal:8102", project_host)
    );
    assert_eq!(
        data["links"]["dsh_url"].as_str().unwrap(),
        format!("http://3080.{}.devvm.localhost:8102", project_host)
    );

    // 2. Launch DSH again -> Idempotent, remains running
    let res = ctx.client.post(&launch_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["dsh_status"], "running");

    // 3. Stop DSH
    let stop_dsh_url = format!(
        "http://{}/api/projects/{}/dsh/stop",
        ctx.server_addr, project_id
    );
    let res = ctx.client.post(&stop_dsh_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["dsh_status"], "stopped");
    assert!(data["links"]["local_dsh_url"].is_null());
    assert!(data["links"]["tailnet_dsh_url"].is_null());
    assert!(data["links"]["dsh_url"].is_null());

    let invocations = fs::read_to_string(project_dir.join(".mock_exec_invocations")).unwrap();
    assert!(invocations.contains("kill -0"));

    // 4. Restart immediately after stop; old DSH must no longer own port 3080
    let res = ctx.client.post(&launch_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["dsh_status"], "running");
    let res = ctx.client.post(&stop_dsh_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 5. Test failure detection: simulate DSH crashing unexpectedly
    fs::write(project_dir.join(".dsh_fail_short"), "1").unwrap();

    let res = ctx.client.post(&launch_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Process will exit after ~300ms
    let mut dsh_status = String::new();
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let res = ctx.client.get(&proj_url).send().await.unwrap();
        let data: Value = res.json().await.unwrap();
        dsh_status = data["dsh_status"].as_str().unwrap_or("").to_string();
        if dsh_status == "failed" {
            break;
        }
    }
    assert_eq!(dsh_status, "failed");

    // Invariant: Failed state does NOT auto-restart; manual restart required
    tokio::time::sleep(Duration::from_millis(300)).await;
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["dsh_status"], "failed");

    // Remove fail trigger and manually restart
    fs::remove_file(project_dir.join(".dsh_fail_short")).unwrap();
    let res = ctx.client.post(&launch_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    tokio::time::sleep(Duration::from_millis(150)).await;
    let res = ctx.client.get(&proj_url).send().await.unwrap();
    let data: Value = res.json().await.unwrap();
    assert_eq!(data["dsh_status"], "running");
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
        format!("http://3000.{}.devvm.internal:8102", project_host)
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
    assert_eq!(data["logs"].as_str().unwrap(), "");
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
    let logs = logs_data["logs"].as_str().unwrap();

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
        "[INFO] [client] [3080.ongoing-proj] start proxy success\n\
         [INFO] [caddy] reverse_proxy: 127.0.0.1:3080 -> loopback upstream connected\n\
         [WARN] [client] heartbeat timeout, reconnecting to frps\n",
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
    let logs = logs_data["logs"].as_str().unwrap();

    assert!(logs.contains("start proxy success"));
    assert!(logs.contains("reverse_proxy: 127.0.0.1:3080 -> loopback upstream connected"));
    assert!(logs.contains("heartbeat timeout, reconnecting to frps"));
    assert!(logs.contains("[ingress]"));
}

#[test]
fn test_ingress_script_no_workspace_pollution() {
    let script_content =
        fs::read_to_string("scripts/devvm-ingress").expect("scripts/devvm-ingress must exist");
    assert!(
        !script_content.contains(".devvm-ingress.log"),
        "scripts/devvm-ingress must not log to .devvm-ingress.log or pollute /root/workspace"
    );
    assert!(
        !script_content.contains("/root/workspace/.devvm-ingress.log"),
        "scripts/devvm-ingress must not write logs inside /root/workspace"
    );
}

#[tokio::test]
async fn test_multiple_listeners_serving_and_tailnet_boundary() {
    use devvm_daemon::determine_bind_addresses;
    use std::net::Ipv4Addr;

    // Verify determine_bind_addresses never produces 0.0.0.0 by default
    let default_addrs = determine_bind_addresses(None, 8100, None);
    assert_eq!(
        default_addrs,
        vec![SocketAddr::from(([127, 0, 0, 1], 8100))]
    );

    let ts_ip: Ipv4Addr = "100.64.0.42".parse().unwrap();
    let dual_addrs = determine_bind_addresses(None, 8100, Some(ts_ip));
    assert_eq!(
        dual_addrs,
        vec![
            SocketAddr::from(([127, 0, 0, 1], 8100)),
            SocketAddr::from(([100, 64, 0, 42], 8100)),
        ]
    );

    // Verify dual listener serving works concurrently
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();

    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let devvm_bin = temp_dir.path().join("mock_devvm");
    create_mock_devvm(&devvm_bin);

    let config = DaemonConfig {
        host: String::new(),
        port: 0,
        config_path: config_dir.join("projects.json"),
        sync_config_path: config_dir.join("sync.json"),
        log_dir,
        home_dir,
        devvm_bin,
        ingress_port: 8102,
        tailnet_domain: "devvm.internal".to_string(),
    };

    let dsh_runtime_manager = DshRuntimeManager::new();
    let sync_manager = SyncManager::new();
    let state = AppState {
        config: config.clone(),
        dsh_runtime_manager,
        sync_manager,
    };

    let router = create_router(state);

    let listener_1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr_1 = listener_1.local_addr().unwrap();

    let listener_2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr_2 = listener_2.local_addr().unwrap();

    let r1 = router.clone();
    tokio::spawn(async move {
        axum::serve(listener_1, r1).await.unwrap();
    });

    let r2 = router;
    tokio::spawn(async move {
        axum::serve(listener_2, r2).await.unwrap();
    });

    let client = reqwest::Client::new();

    // Query listener 1
    let res_1 = client
        .get(format!("http://{}/api/projects", addr_1))
        .send()
        .await
        .unwrap();
    assert_eq!(res_1.status(), StatusCode::OK);

    // Query listener 2
    let res_2 = client
        .get(format!("http://{}/api/projects", addr_2))
        .send()
        .await
        .unwrap();
    assert_eq!(res_2.status(), StatusCode::OK);
}
