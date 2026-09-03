# 07: Surface Control Daemon errors faithfully

**What to build:** Every error the Control Daemon encounters reaches its process log (stderr via `tracing`) with the full underlying message, and project-scoped errors also reach that Project's `daemon.log`. No preflight checks; observability comes from never dropping an error.

**Blocked by:** none

**Status:** resolved

**Why:** A `devvm` binary that had lost its executable bit made every action fail, yet the daemon's stderr stayed empty: `check_vm_status` maps `Err(_)` to `VmStatus::Stopped`, handlers return errors only as HTTP JSON, several call sites use `let _ =`, and the `tower_http=info` filter names a `TraceLayer` that was never installed.

## Rules

- Every `Err` produced in `src/api.rs`, `src/runner.rs`, `src/runtime.rs`, `src/sync.rs`, `src/dns.rs`, `src/registry.rs`, `src/browser.rs` is logged with `tracing::error!` before being converted to an HTTP response or swallowed. The event carries: route or operation name, Project ID when known, and the complete error text.
- Failed external commands (`devvm`, `ssh`, `smolvm`) log the program, arguments, exit code, and captured stdout and stderr verbatim.
- `check_vm_status`: `Err(e)` logs `error!`; a zero exit with stdout that is neither `running` nor `stopped` logs `warn!` with the stdout.
- Project-scoped errors are also appended to the Project's `daemon.log` (see issue 09 for the log location; until 09 lands, the current `append_log` path) so the UI log viewer shows them.
- One helper in `src/api.rs`, `fn api_error(status: StatusCode, project: Option<Uuid>, context: &str, error: impl Display) -> Response`, does the logging and the JSON body; every handler error path uses it.
- `tower-http` `TraceLayer::new_for_http()` is added to the router. Add the `tower-http` dependency with the `trace` feature if absent.
- Default `EnvFilter` stays `devvm_daemon=info,tower_http=info`.

## Acceptance criteria

- [x] With `DEVVM_BIN` pointing at a non-executable file, `POST /api/projects/{id}/vm/start` returns 500 and the daemon stderr contains the OS error text (`Permission denied`) and the command path.
- [x] `check_vm_status` against a failing `devvm` emits a `tracing` error event containing the command's stderr.
- [x] Every request produces a `tower_http` trace line at `info`.
- [x] Project-scoped errors appear in the Project's `daemon.log` with the same text as the HTTP error body.
- [x] Tests capture tracing events with a `tracing_subscriber` in-memory writer (no hand-rolled mocks) and assert the error text is present.
- [x] `.agents/lessons.md` records: never rewrite executables in this repo with the `write` tool; after any edit to `devvm` or `scripts/*`, re-check the mode.

## Answer

Changed:

- `src/api.rs`: added `api_error(status, log_dir, project, context, error)` which emits `tracing::error!(project, context, error, "request failed")`, appends the same text to the Project's daemon log via `append_log` when the Project is known, and renders the unchanged `{"error": ...}` body. Every handler error path now goes through it; `TraceLayer::new_for_http()` (span and response events raised to `INFO`) is layered on the router.
- `src/runner.rs`: shared `log_command_failure` / `log_command_spawn_failure`; `check_vm_status` logs io errors (program, args, error) before returning `Stopped`, `warn!`s unrecognized stdout on a zero exit, and `error!`s exit code plus stdout/stderr on a non-zero exit; `devvm` subcommand failures log the same way.
- `src/logs.rs`: `append_log_logged` replaces the `let _ = append_log(...)` pattern and reports write failures.
- `src/runtime.rs`: spawn/exit failures of `devvm exec` log program, args, exit code, stdout, stderr; all `let _ =` sites now log (`kill_child`, `send_startup_result`, `send_stop_result`, project-log appends, `handle_vm_stopped`).
- `src/sync.rs`: `ssh_args` makes the ssh argument vector loggable; `verify`, `delete_store` and `read_status` log failures verbatim.
- `src/dns.rs`: `detect_tailscale_ipv4` logs spawn failure and non-zero exit; DNS send and shutdown-watch errors are logged instead of dropped.
- `tests/observability_test.rs` (new): in-memory `MakeWriter` + `tracing::subscriber::set_default`, router driven via `tower::ServiceExt::oneshot`; covers non-executable `devvm` (500 + `Permission denied` + command path), failing `devvm status` stderr, a `tower_http` line at `info`, and the project log carrying the HTTP body's error text.
- `tests/common/mod.rs`: `.vm_status_fail` hook makes the mock `devvm status` exit non-zero with stderr.
- `.agents/lessons.md`: executable-mode lesson appended.

Verified: `cargo build` clean; `cargo clippy --all-targets -- -D warnings` zero warnings; `cargo test` 78 passed, 0 failed, 1 ignored (the live acceptance test stays ignored). Each new assertion was inverted once and observed failing, then restored (file byte-identical). Hosting `dsh web` PID 156 unchanged before and after; `ls -l devvm scripts/` modes unchanged.

Deviations:

- `api_error` takes `log_dir: &Path` in addition to the ticket signature, because `append_log` needs the log directory and the helper has no access to `AppState`.
- Errors raised while a Project cannot be resolved (`Project not found`, registry read failure) are logged with `project = None`; writing them into a log file for an unregistered id created log output for non-existent Projects and broke the existing `test_non_existent_project_operations` expectation.
- `SystemSyncRunner::read_status` logs its non-zero exit at `debug!`: `cat ... 2>/dev/null` exits non-zero whenever no Sync Status file exists yet, which is the ordinary state and not an error.
- The `TraceLayer` span/response levels are set to `INFO` so each request produces a `tower_http` line under `tower_http=info`; the tower-http defaults are `DEBUG` and emit nothing at that filter.
- The non-executable `devvm` test sets `DaemonConfig.devvm_bin` directly rather than the `DEVVM_BIN` env var, to avoid the process-wide env races recorded in `.agents/lessons.md`.

## Comments

Orchestrator mutation audit: disabling `api_error`'s tracing event or its `daemon.log` append left every test green, because `run_devvm_command` wrote its own `[daemon:error]` line and the tests only checked for presence. The same failure therefore appeared twice in `daemon.log`, the second copy tagged `[daemon]` (level info). Fixed: `api_error` is the only handler-error writer and tags `daemon:error`; the runner and sync paths no longer append their own error lines; tests assert exactly-once and the error tag, plus a new handler-only test (`sync/delete` without confirmation) that exercises `api_error` with no command behind it. All fourteen mutants now fail at least one test.
