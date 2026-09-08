#![allow(dead_code)]

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Child;

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

/// Creates a mock `ssh` executable that succeeds unless the Sync Store looks unreachable.
/// Placed beside the mock `devvm` binary, which is where `SystemSyncRunner` looks for ssh.
pub fn create_mock_ssh(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
if [[ -f ".mock_ssh_fail" ]] || [[ "$*" == *"127.0.0.1:1"* ]] || [[ "$*" == *"-p 1 "* ]]; then
    echo "Mock SSH: connection refused" >&2
    exit 255
fi
exit 0
"#;
    let mut file = File::create(bin_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    let mut perms = file.metadata().unwrap().permissions();
    perms.set_mode(0o755);
    file.set_permissions(perms).unwrap();
}

fn write_executable(path: &Path, script: &str) {
    let mut file = File::create(path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    let mut perms = file.metadata().unwrap().permissions();
    perms.set_mode(0o755);
    file.set_permissions(perms).unwrap();
}

/// Creates a mock executable bash script for `devvm` CLI that simulates VM lifecycle and
/// evaluates guest command snippets with the absolute guest paths remapped into the test's
/// own directories, so `/tmp/devvm-daemon-dsh.pid` on the host is never touched.
///
/// `log_dir` is the daemon's Project log directory, which the guest sees as
/// `/devvm-root/.project-logs`.
pub fn create_mock_devvm(bin_path: &Path, log_dir: &Path) {
    let guest_bin = bin_path.parent().unwrap().join("mock_guest_bin");
    std::fs::create_dir_all(&guest_bin).unwrap();
    // Stands in for `dsh web`: announces its URL, records the start, then blocks until killed.
    write_executable(
        &guest_bin.join("dsh"),
        r#"#!/usr/bin/env bash
if [[ "${1:-}" == "web" ]]; then
    shift
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --no-open) shift ;;
            *) echo "dsh web: unknown option $1" >&2; exit 1 ;;
        esac
    done
    printf 'start\n' >> "$MOCK_DSH_START_COUNTER"
    count=$(wc -l < "$MOCK_DSH_START_COUNTER")
    token="mock-token-redacted-${count}-base64url-auth-token00"
    echo "dsh web: http://127.0.0.1:3080/?token=${token}"
    exec sleep 300
fi
exit 0
"#,
    );
    write_executable(
        &guest_bin.join("devvm-sync-startup"),
        r#"#!/usr/bin/env bash
echo "devvm-sync-startup: startup reconciliation done"
exit 0
"#,
    );

    let script = r#"#!/usr/bin/env bash
cmd="${1:-status}"
shift || true

case "$cmd" in
    status)
        if [[ -f ".vm_status_fail" ]]; then
            echo "Mock DevVM: status probe failed hard" >&2
            exit 3
        fi
        if [[ -f ".vm_running" ]]; then
            echo "Machine 'test-project': running"
            exit 0
        else
            echo "stopped"
            exit 1
        fi
        ;;
    start)
        if [[ -f ".vm_start_slow" ]]; then
            sleep 0.4
        fi
        touch ".vm_running"
        echo "Mock DevVM: started"
        exit 0
        ;;
    stop)
        rm -f ".vm_running"
        echo "Mock DevVM: stopped"
        exit 0
        ;;
    rm|delete)
        rm -f ".vm_running"
        echo "Mock DevVM: removed"
        exit 0
        ;;
    exec)
        if [[ "${1:-}" == "--" ]]; then
            shift
        fi

        VM_DSH="$PWD/.mock_dsh"
        mkdir -p "$VM_DSH"
        VM_RUN="$PWD/.mock_run"
        mkdir -p "$VM_RUN"

        echo "$@" >> "$PWD/.mock_exec_invocations" 2>/dev/null || true

        if [[ "$1" == "/bin/sh" || "$1" == "/bin/bash" ]] && [[ "$2" == "-c" ]]; then
            cmd_body="$3"
            if [[ -f ".dsh_start_fail" && "$cmd_body" == *"exec dsh web"* ]]; then
                echo "Mock DevVM: dsh could not be started" >&2
                exit 7
            fi
            mapped_cmd="${cmd_body//\/tmp\/devvm-daemon-dsh.pid/$PWD/.mock_dsh.pid}"
            mapped_cmd="${mapped_cmd//\/tmp\/devvm-daemon-dsh.token/$PWD/.mock_dsh.token}"
            mapped_cmd="${mapped_cmd//\/devvm-root\/.project-logs/__LOG_DIR__}"
            mapped_cmd="${mapped_cmd//\/run\/devvm/$VM_RUN}"
            mapped_cmd="${mapped_cmd//\/root\/workspace/$PWD}"
            mapped_cmd="${mapped_cmd//\/root\/.dsh/$VM_DSH}"
            PATH="__GUEST_BIN__:$PATH" \
                MOCK_DSH_START_COUNTER="$PWD/.mock_dsh_starts" \
                bash -c "$mapped_cmd"
            exit $?
        fi

        if [[ "$1" == "rsync" ]]; then
            echo "Mock DevVM: the daemon must not invoke rsync" >&2
            exit 1
        fi

        echo "Mock DevVM exec: $@"
        exit 0
        ;;
    *)
        echo "Unknown command: $cmd" >&2
        exit 1
        ;;
esac
"#
    .replace("__LOG_DIR__", &log_dir.display().to_string())
    .replace("__GUEST_BIN__", &guest_bin.display().to_string());

    write_executable(bin_path, &script);
}

/// Number of times the mock `dsh web` actually started for a Project.
pub fn mock_dsh_start_count(project_dir: &Path) -> usize {
    std::fs::read_to_string(project_dir.join(".mock_dsh_starts"))
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

/// The isolated stand-in for the guest's `/tmp/devvm-daemon-dsh.pid`.
pub fn mock_dsh_pid_file(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".mock_dsh.pid")
}

/// The isolated stand-in for the guest's `/tmp/devvm-daemon-dsh.token`.
pub fn mock_dsh_token_file(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".mock_dsh.token")
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
