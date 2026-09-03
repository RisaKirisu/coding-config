# 09: Make the DevVM the source of truth for the DSH Runtime

**What to build:** The Control Daemon starts, stops, and probes the DSH Runtime with three `devvm exec` commands and holds no child process. DSH runs detached inside the DevVM and writes its own timestamped log to a host-shared Project log directory. DSH Status survives daemon restarts because it is read from the DevVM, never from daemon memory.

**Blocked by:** 07

**Status:** resolved

**Why:** Today the daemon spawns `devvm exec ... exec dsh web` as its own child, pumps the child's stdout and stderr into the Project Log, greps for readiness, and `select!`s on stop-versus-exit. When the daemon restarts, the exec stream dies, `dsh web` keeps running inside the VM, and the daemon reports it stopped. Logs also stop flowing. Ingress already solved this differently (pid files, `nohup`); this ticket unifies on one simple model.

## Log layout and line format

One host directory per Project: `<DEVVM_ROOT>/.project-logs/<project-id>/` containing `daemon.log` (written by the daemon on the host), `dsh.log` (written inside the VM), `ingress.log` (written inside the VM; moved from `.ingress-logs`). Visible in the guest at `/devvm-root/.project-logs/<project-id>/`.

Every line in every file: `[<ISO-8601 UTC with milliseconds>] <text>`, for example `[2026-09-02T22:15:16.210Z] dsh web: http://127.0.0.1:3080`. All writers are unbuffered per line.

- Guest prefixer (used by `dsh` and `frpc`): `while IFS= read -r line; do printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)" "$line"; done`. Confirm `date +%3N` works in the image (GNU coreutils); fall back to `%N` truncated if not.
- Caddy keeps its JSON lines (already one per line, unbuffered); the daemon converts `ts` at read time (issue 10).
- Daemon `append_log` writes `[ISO] [source] text` where `source` stays `daemon`, `daemon:error`, `sync`, etc.; open-append-write-flush per call.
- `DaemonConfig.log_dir` becomes `<DEVVM_ROOT>/.project-logs`; `DEVVM_LOG_DIR` override kept. `ingress_log_candidates` and its six probes are deleted.
- `scripts/devvm-ingress`: `HOST_LOG_DIR=/devvm-root/.project-logs/$PROJECT_ID`; frpc output passes through the prefixer; caddy unchanged.

## Daemon commands (constants in `src/runtime.rs`, run with `devvm exec /bin/bash -c`)

Start (idempotent; `PROJECT_ID` substituted):
```
pid_file=/tmp/devvm-daemon-dsh.pid
if [ -s "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then exit 0; fi
log_dir=/devvm-root/.project-logs/<project-id>
install -d -m 0700 "$log_dir"
setsid bash -c '
  echo $$ > /tmp/devvm-daemon-dsh.pid
  cd /root/workspace && devvm-sync-startup
  exec dsh web
' </dev/null 2>&1 | <prefixer> >> "$log_dir/dsh.log" &
```
(Arrange the pipeline so the pid recorded is `dsh`'s own process, not the prefixer's: run `setsid bash -c '...' 2>&1 </dev/null | prefixer >> log &` with `echo $$` inside the inner bash, as shown; verify with `devvm exec cat /tmp/devvm-daemon-dsh.pid` and `ps`.)

Stop: the existing `DSH_STOP_COMMAND` (TERM, wait up to 5 s, KILL, remove pid file), unchanged.

Status: `[ -s /tmp/devvm-daemon-dsh.pid ] && kill -0 "$(cat /tmp/devvm-daemon-dsh.pid)"`.

## `DshRuntimeManager`

- `get_status(project)`: if an in-flight operation is recorded → `Starting` or `Stopping`; else if VM not running → `Stopped`; else run the status command, cache the result for 2 s → `Running` or `Stopped`. `DshStatus::Failed` is removed (a crash is `Stopped`; the cause is in `dsh.log`).
- `launch_dsh`: refuse while an operation is in flight; record `Starting`; ensure VM running (existing `prepare_launch` VM-start logic); run the start command; log the command's stderr through issue 07's error path on non-zero exit; clear in-flight.
- `stop_dsh`: record `Stopping`; run stop command; clear in-flight.
- Restart handler unchanged (stop then launch).
- ADR 0004 invariant holds through the idempotent start: a second launch never starts a second `dsh web` and never runs reconciliation under a running one.
- Delete: `ProcessState`, `StopRequestSender`, child handle, stdout/stderr pump tasks, readiness grep, `DSH_STARTUP_TIMEOUT`, `record_unexpected_exit`, `stop_managed_process`.

## Behaviour changes

- The DSH link appears once the process is alive, not once it is listening; an immediate click may see a brief 502 from ingress.
- A crashed DSH shows `stopped`; `dsh.log` holds the reason.
- DSH and ingress logs persist across daemon restarts and VM restarts (host directory).

## Acceptance criteria

- [x] Launch DSH, construct a fresh `DshRuntimeManager` over the same config, `get_status` is `running`; stop → `stopped`. Same via the HTTP API after a simulated daemon restart in tests.
- [x] Second launch while running does not spawn a second process (mock records one start).
- [x] Start command failure returns 500 with the command's stderr in the body, daemon stderr, and `daemon.log`.
- [x] `dsh.log` lines carry the `[ISO]` prefix; the mock `devvm` writes a `dsh.log` in the same format so `logs.rs` tests exercise it.
- [x] `scripts/devvm-ingress` writes `ingress.log` under `.project-logs`; frpc lines are prefixed; `bash -n` passes.
- [x] Lifecycle test fakes isolate guest pid files under the project's temp dir (AGENTS.md rule); the hosting DSH PID is unchanged before and after the suite.
- [x] `readme.md`, `AGENTS.md` runtime-verification section, and ADR 0004 (restart wording) updated; `DshStatus::Failed` removed from `models.rs` and `ui.rs`.

## Verification requiring a real DevVM host

Launch DSH from the UI; restart the daemon; UI shows `DSH: Running` and the link works; `cat root/.project-logs/<id>/dsh.log` on the host shows prefixed lines appearing live during a turn; Stop DSH from the UI stops the guest process (`devvm exec pgrep -f 'dsh web'` empty).

## Answer

What changed:

- `src/runtime.rs`: rewritten around three guest snippets (`DSH_START_COMMAND` with a `{project_id}` placeholder substituted via `str::replace`, `DSH_STATUS_COMMAND`, the unchanged `DSH_STOP_COMMAND`), each run as `devvm exec /bin/bash -c <snippet>` through `tokio::process::Command::output()`. `DshRuntimeManager` now holds only `HashMap<Uuid, InFlight>` (Starting/Stopping) plus a `HashMap<Uuid, (Instant, DshStatus)>` status cache with a 2 s TTL. `get_status(config, project_id, project_path)`: in-flight wins, a non-running DevVM is `Stopped`, otherwise a cached or fresh guest probe. `launch_dsh` refuses while an operation is in flight, ensures the DevVM is running, runs the idempotent start snippet, then clears the in-flight entry and invalidates the cache; `stop_dsh` is symmetrical. `ProcessState`, `StopRequestSender`, the child handle, the stdout/stderr pump tasks, the readiness grep, `DSH_STARTUP_TIMEOUT`, `record_unexpected_exit`, `stop_managed_process`, `kill_child`, and the oneshot plumbing are gone.
- `src/logs.rs`: `project_log_dir` / `daemon_log_path` replace `project_log_path`; `append_log` writes `<log_dir>/<project-id>/daemon.log` with `[<ISO-8601 UTC ms>] [<source>] <text>` using a 20-line `format_iso8601_millis` over `std::time::SystemTime` (no new crate; the crate registry is offline here); `read_recent_logs` concatenates the tails of `daemon.log`, `dsh.log`, `ingress.log` in that order and keeps the `{project_id, logs}` API shape. `ingress_log_candidates` and its six probe groups are deleted.
- `src/config.rs`: `DaemonConfig.log_dir` defaults to `<DEVVM_ROOT>/.project-logs` (reusing `sync::devvm_root`, now `pub(crate)`), `DEVVM_LOG_DIR` override kept.
- `src/models.rs`: `DshStatus::Failed` removed and `DshStatus` is now `Copy`; a crash reads back as `Stopped`.
- `src/api.rs`: `get_status`, `stop_dsh`, and `handle_vm_stopped` take the Project path; `stop_dsh_handler` resolves the Project first (404 for an unknown id).
- `scripts/devvm-ingress`: `HOST_LOG_DIR=/devvm-root/.project-logs/$PROJECT_ID`, new `prefix_lines()` helper, and frpc's output goes through process substitution (`> >(prefix_lines >> "$INGRESS_LOG")`) so the pid file still holds frpc's own pid. Caddy is unchanged and the file mode is still `-rw-r--r--`.
- `tests/common/mod.rs`: `create_mock_devvm(bin_path, log_dir)` rewrites the guest paths `/tmp/devvm-daemon-dsh.pid`, `/devvm-root/.project-logs`, `/run/devvm`, `/root/workspace`, `/root/.dsh` into the test's own directories and runs the real snippet with real `bash`, with `dsh` and `devvm-sync-startup` stubs on a mock-owned PATH (`dsh web` prints its URL, records a start, then `exec sleep 300`). New helpers `mock_dsh_start_count` and `mock_dsh_pid_file`; new `.dsh_start_fail` hook. The old `dsh web` interception and the pid-file special case are gone.
- `tests/api_test.rs`: DSH tests replaced by `test_dsh_status_is_read_from_the_devvm_and_survives_a_daemon_restart` (fresh manager and a second router over the same config both read `running`, then `stopped`; `dsh.log` lines all match the ISO prefix; the log viewer shows them), `test_second_launch_does_not_spawn_a_second_dsh_process` (start counter is 1, pid unchanged), `test_dsh_restart_replaces_the_running_process` (new pid, 2 starts), and `test_dsh_start_failure_reports_the_command_stderr` (500, stderr in body and in `daemon.log`, no start recorded, status back to `stopped`). The startup-timeout and stop-during-start tests are deleted with the behaviour they covered.
- `tests/acceptance_workflow_test.rs`, `tests/live_acceptance_test.rs` (still `#[ignore]`d), `tests/observability_test.rs`, `tests/sync_test.rs`: fixed-sleep waits replaced by bounded polling for the detached DSH, `daemon_log_path` import, mock signature.
- Docs: `readme.md` (log directory, line format, `running` means alive, crash reads back as `stopped`), `AGENTS.md` runtime verification (three snippets, log layout, which guest paths the fakes remap), ADR 0004 (the idempotent launch, not a refusal, is what upholds stop-before-pull), `CONTEXT.md` (Project Log is one directory with three writers; DSH Status is read from the DevVM), `.gitignore` (`root/.project-logs/`).

Verified here: `cargo build` clean; `cargo clippy --all-targets -- -D warnings` zero warnings; `cargo test` 83 passed, 0 failed, 1 ignored; `bash -n scripts/devvm-ingress` passes and `ls -l devvm scripts/` shows unchanged modes (`devvm` `-rwxr-xr-x`, `scripts/devvm-ingress` `-rw-r--r--`). The hosting DSH pid stayed 156 and alive across every run, `/tmp/devvm-daemon-dsh.pid` on the host still reads `156` with its original mtime, and no `sleep 300` survives a clean suite run. Four new assertions were inverted once and observed failing, then restored byte-identically (idempotent start guard, guest prefixer, guest probe result, `api_error`'s project-log append).

Deviations:

- The in-flight rejection returns the ordinary launch error, which `launch_dsh_handler` renders as 500: the API has no per-error status mapping and no existing "already starting" 409/400 branch, and adding one would mean matching on error strings.
- The start snippet ends the prefixer with `>> "$log_dir/dsh.log" 2>&1` (the ticket leaves the prefixer's stderr inherited). Without it the backgrounded prefixer keeps the `devvm exec` stderr pipe open, and `Command::output()` blocks until DSH exits.
- `stop_dsh` (and therefore `handle_vm_stopped`) returns `Ok(())` without running anything when the DevVM is not running, because `devvm exec` would create and start a DevVM just to look for a DSH that cannot exist. `get_status` short-circuits the same way, as the ticket specifies.
- `read_recent_logs` still tags `dsh.log` lines with `[dsh] ` and `ingress.log` lines with `[ingress] ` while concatenating. Issue 10 owns merging by timestamp; the tags keep the existing viewer assertions meaningful in the meantime.
- `.badge-failed` stays in `src/ui.rs`: it is shared with `VmStatus::Failed`, which this ticket does not touch. No DSH-specific `failed` rendering exists.
- The ISO formatter is hand-rolled (18 lines plus a test over four epochs including a leap day and 2100-03-01) because no `time`/`humantime` crate is vendored and the registry is unreachable from this machine.
- DSH Status becomes `running` on the first poll after the launch call, not inside it: the start snippet returns as soon as the detached pipeline is backgrounded, so tests poll instead of asserting immediately.
- The stale `root/.ingress-logs/` data from earlier runs was left in place (and still ignored) rather than moved or deleted.

Not verified here — requires a real DevVM host (`smolvm` is not installed on this machine):

- Launching DSH from the UI, restarting the Control Daemon, and seeing `DSH: Running` with a working link.
- `cat root/.project-logs/<id>/dsh.log` on the host showing prefixed lines appear live during a DSH turn.
- Stop DSH from the UI actually stopping the guest process (`devvm exec pgrep -f 'dsh web'` empty).
- `devvm exec cat /tmp/devvm-daemon-dsh.pid` plus `ps` inside the DevVM confirming the pid is DSH's own and not the prefixer's, and that `devvm exec` returns promptly despite the backgrounded pipeline.
- `scripts/devvm-ingress` inside a running DevVM writing `ingress.log` under `/devvm-root/.project-logs/<id>/` with prefixed frpc lines, `date -u +%3N` producing milliseconds in the `python:3.14-slim-trixie` image, and `/run/devvm/frpc.pid` holding frpc's pid.
- DSH and ingress logs surviving a DevVM restart.
