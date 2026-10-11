mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{exited, is_devvm, ready, verify, MockRunner, DEVVM};
use devvm_daemon::logs::daemon_log_path;
use devvm_daemon::models::VmStatus;
use devvm_daemon::registry::register_project;
use devvm_daemon::{
    create_router, AppState, CommandRunner, DaemonConfig, Devvm, ProcessCommandRunner,
};
use http_body_util::BodyExt;
use mockall::Sequence;
use serde_json::Value;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::{tempdir, TempDir};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

/// In-memory `MakeWriter` collecting everything the test's tracing subscriber emits.
#[derive(Clone, Default)]
struct CapturedTracing(Arc<Mutex<Vec<u8>>>);

impl CapturedTracing {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

impl io::Write for CapturedTracing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedTracing {
    type Writer = CapturedTracing;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn capture_tracing(filter: &str) -> (CapturedTracing, tracing::subscriber::DefaultGuard) {
    let captured = CapturedTracing::default();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_writer(captured.clone())
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    (captured, guard)
}

struct Ctx {
    _temp_dir: TempDir,
    config: DaemonConfig,
    project_id: Uuid,
    project_path: PathBuf,
}

fn setup() -> Ctx {
    let temp_dir = tempdir().unwrap();
    let home_dir = temp_dir.path().join("home");
    let project_path = home_dir.join("project");
    fs::create_dir_all(&project_path).unwrap();

    let config_dir = temp_dir.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("projects.json");
    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).unwrap();

    let record = register_project(&config_path, project_path.to_str().unwrap()).unwrap();

    Ctx {
        _temp_dir: temp_dir,
        config: DaemonConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            config_path,
            sync_config_path: config_dir.join("sync.json"),
            log_dir,
            home_dir,
            devvm_bin: PathBuf::from(DEVVM),
            ingress_port: 8102,
            remote_domain: None,
        },
        project_id: record.id,
        project_path,
    }
}

fn router(ctx: &Ctx, runner: Arc<dyn CommandRunner>) -> axum::Router {
    create_router(AppState::new(ctx.config.clone(), runner))
}

async fn start_vm(ctx: &Ctx, runner: Arc<dyn CommandRunner>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/projects/{}/vm/start", ctx.project_id))
        .body(Body::empty())
        .unwrap();
    let response = router(ctx, runner).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn test_non_executable_devvm_logs_permission_denied() {
    let mut ctx = setup();
    ctx.config.devvm_bin = ctx._temp_dir.path().join("devvm");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("devvm"),
        &ctx.config.devvm_bin,
    )
    .unwrap();
    fs::set_permissions(&ctx.config.devvm_bin, fs::Permissions::from_mode(0o644)).unwrap();
    let (captured, _guard) = capture_tracing("devvm_daemon=info");

    let (status, body) = start_vm(&ctx, Arc::new(ProcessCommandRunner)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Permission denied"));

    let logged = captured.text();
    assert!(
        logged.contains("Permission denied"),
        "tracing output missing OS error text: {}",
        logged
    );
    assert!(
        logged.contains(ctx.config.devvm_bin.to_str().unwrap()),
        "tracing output missing command path: {}",
        logged
    );
}

#[tokio::test]
async fn test_failing_status_command_logs_stderr() {
    let ctx = setup();
    let mut runner = MockRunner::new();
    let mut sequence = Sequence::new();
    let dir = ctx.project_path.clone();
    runner
        .expect_run()
        .withf(move |command, _| is_devvm(command, &dir, &["status"]))
        .times(1)
        .in_sequence(&mut sequence)
        .returning(|_, _| {
            ready(exited(
                1,
                "",
                "Failed to list machines: connection refused\n",
            ))
        });
    let runner = Arc::new(runner);
    let (captured, _guard) = capture_tracing("devvm_daemon=info");

    let devvm = Devvm::new(ctx.config.devvm_bin.clone(), runner.clone());
    assert_eq!(
        devvm
            .status(&ctx.project_path, &CancellationToken::new())
            .await,
        VmStatus::Stopped
    );
    drop(devvm);
    verify(runner);

    let logged = captured.text();
    assert!(
        logged.contains("Failed to list machines: connection refused"),
        "tracing output missing command stderr: {}",
        logged
    );
}

#[tokio::test]
async fn test_request_produces_tower_http_trace_line() {
    let ctx = setup();
    let mut runner = MockRunner::new();
    let mut sequence = Sequence::new();
    let dir = ctx.project_path.clone();
    runner
        .expect_run()
        .withf(move |command, _| is_devvm(command, &dir, &["status"]))
        .times(1)
        .in_sequence(&mut sequence)
        .returning(|_, _| ready(exited(0, "", "")));
    let runner = Arc::new(runner);
    let (captured, _guard) = capture_tracing("devvm_daemon=info,tower_http=info");

    let request = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();
    let response = router(&ctx, runner.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    verify(runner);
    let logged = captured.text();
    assert!(
        logged.contains("tower_http"),
        "tracing output missing tower_http trace line: {}",
        logged
    );
}

#[tokio::test]
async fn test_project_scoped_error_reaches_project_log() {
    let ctx = setup();
    let mut runner = MockRunner::new();
    let mut sequence = Sequence::new();
    let dir = ctx.project_path.clone();
    runner
        .expect_run()
        .withf(move |command, _| is_devvm(command, &dir, &["start"]))
        .times(1)
        .in_sequence(&mut sequence)
        .returning(|_, _| {
            ready(exited(
                1,
                "",
                "Failed to start machine: insufficient memory\n",
            ))
        });
    let runner = Arc::new(runner);
    let (_captured, _guard) = capture_tracing("devvm_daemon=info");

    let (status, body) = start_vm(&ctx, runner.clone()).await;
    verify(runner);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let error_text = body["error"].as_str().unwrap();
    assert!(error_text.contains("Failed to start machine: insufficient memory"));

    let log_contents =
        fs::read_to_string(daemon_log_path(&ctx.config.log_dir, ctx.project_id)).unwrap();
    let error_lines: Vec<&str> = log_contents
        .lines()
        .filter(|line| line.contains(error_text))
        .collect();
    assert_eq!(
        error_lines.len(),
        1,
        "the HTTP error text must appear exactly once in the project log: {}",
        log_contents
    );
    assert!(
        error_lines[0].contains("] [daemon:error] "),
        "the error line must carry the error level tag: {}",
        error_lines[0]
    );
}

/// An error raised by the handler itself, with no external command behind it, still reaches
/// tracing and the Project's `daemon.log` through `api_error` alone.
#[tokio::test]
async fn test_handler_only_error_is_logged_once_at_error_level() {
    let ctx = setup();
    let runner = Arc::new(MockRunner::new());
    let (captured, _guard) = capture_tracing("devvm_daemon=info");

    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/projects/{}/sync/delete", ctx.project_id))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"confirmed":false}"#))
        .unwrap();
    let response = router(&ctx, runner.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let error_text = body["error"].as_str().unwrap();

    verify(runner);
    let logged = captured.text();
    assert!(
        logged.contains("ERROR") && logged.contains(error_text),
        "tracing output missing the handler error: {}",
        logged
    );

    let log_contents =
        fs::read_to_string(daemon_log_path(&ctx.config.log_dir, ctx.project_id)).unwrap();
    let error_lines: Vec<&str> = log_contents
        .lines()
        .filter(|line| line.contains(error_text))
        .collect();
    assert_eq!(error_lines.len(), 1, "project log: {}", log_contents);
    assert!(
        error_lines[0].contains("] [daemon:error] "),
        "{}",
        error_lines[0]
    );
}
