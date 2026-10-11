#![allow(dead_code)]

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use devvm_daemon::{CommandFuture, CommandRunner, CommandSpec};
use serde_json::json;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, ExitStatus, Output};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Flattens the `entries` of `GET /api/projects/{id}/logs` into `[source] message` lines,
/// for substring assertions over the merged Project Log.
pub fn log_entries_text(logs: &serde_json::Value) -> String {
    logs["entries"]
        .as_array()
        .expect("logs response must carry an entries array")
        .iter()
        .map(|entry| {
            format!(
                "[{}] {}",
                entry["source"].as_str().unwrap_or_default(),
                entry["message"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

mockall::mock! {
    /// Mock of the daemon's command boundary: tests state each `devvm`/`ssh` command they
    /// expect and the documented output it returns.
    pub Runner {}
    impl CommandRunner for Runner {
        fn run(&self, command: CommandSpec, cancel: CancellationToken) -> CommandFuture;
    }
}

/// The program name tests configure as `DaemonConfig::devvm_bin`.
pub const DEVVM: &str = "devvm";

/// A completed command's captured output with exit `code`.
pub fn exited(code: i32, stdout: &str, stderr: &str) -> io::Result<Output> {
    Ok(Output {
        status: ExitStatus::from_raw(code << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    })
}

/// A command future that resolves immediately with `result`.
pub fn ready(result: io::Result<Output>) -> CommandFuture {
    Box::pin(async move { result })
}

/// `devvm status` output for a machine in `state`.
pub fn devvm_status_output(state: &str) -> io::Result<Output> {
    exited(0, &format!("Machine 'dev-project-00000000': {state}\n"), "")
}

/// Whether `command` is `devvm <args>` run from `project_dir`.
pub fn is_devvm(command: &CommandSpec, project_dir: &Path, args: &[&str]) -> bool {
    command.program == Path::new(DEVVM)
        && command.args == args
        && command.current_dir.as_deref() == Some(project_dir)
}

/// Whether `command` is `devvm exec /bin/bash -c <snippet>` from `project_dir` with a
/// snippet containing `marker`.
pub fn is_guest_exec(command: &CommandSpec, project_dir: &Path, marker: &str) -> bool {
    command.program == Path::new(DEVVM)
        && command.current_dir.as_deref() == Some(project_dir)
        && command.args.len() == 4
        && command.args[..3] == ["exec", "/bin/bash", "-c"]
        && command.args[3].contains(marker)
}

/// Markers that identify the daemon's DSH guest snippets.
pub const DSH_LAUNCH: &str = "exec dsh web --no-open";
pub const DSH_STATUS: &str = "[ -s /tmp/devvm-daemon-dsh.token ] || exit 2";
pub const DSH_STOP: &str = "did not stop";

/// Checks every expectation once the server and its tasks have released the mock.
pub fn verify(runner: Arc<MockRunner>) {
    match Arc::try_unwrap(runner) {
        Ok(mut runner) => runner.checkpoint(),
        Err(_) => panic!("the command runner is still referenced; stop the server first"),
    }
}

/// RAII Guard ensuring spawned child processes (like Caddy) are killed and reaped upon drop.
pub struct CaddyGuard(pub Option<Child>);

impl Drop for CaddyGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Axum handler that returns received HTTP headers in JSON response body.
pub async fn echo_headers_handler(headers: HeaderMap) -> impl IntoResponse {
    let mut map = serde_json::Map::new();
    for (k, v) in headers.iter() {
        if let Ok(v_str) = v.to_str() {
            map.insert(k.as_str().to_string(), json!(v_str));
        }
    }
    (StatusCode::OK, Json(map))
}
