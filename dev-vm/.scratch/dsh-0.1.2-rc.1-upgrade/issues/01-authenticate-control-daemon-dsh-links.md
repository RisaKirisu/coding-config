# 01: Authenticate Control Daemon DSH links

**What to build:** Capture the launch token emitted by DSH 0.1.2-rc.1, expose authenticated local and tailnet Project URLs, and keep DSH launch non-interactive.

**Blocked by:** None

**Status:** resolved

## Requirements

- [x] Launch DSH with `dsh web --no-open`.
- [x] Capture only the current DSH launch token from the startup URL emitted by `dsh web`.
- [x] Store the token in a per-Project runtime file that survives a Control Daemon restart while the DSH Runtime remains alive. The file must not be returned by the Project Logs API.
- [x] Remove or replace stale token state before a new launch and remove it when DSH stops.
- [x] When DSH is running and the current token is available, append `?token=<token>` to `local_dsh_url`, `tailnet_dsh_url`, and the compatibility `dsh_url` alias.
- [x] Never return a stale token for a stopped runtime. During the short interval between process start and token emission, omit the DSH link rather than returning a known-unauthenticated URL.
- [x] Preserve idempotent repeated launch, stop/relaunch, PID status probing, DSH status readback after a simulated Control Daemon restart, timestamped `dsh.log`, and existing error reporting.
- [x] Do not add `--trusted-domain` or `--trusted-host`. Keep the localhost-subdomain patch and Caddy Loopback Facade unchanged.

## Verification

- [x] Update the real lifecycle mock so its `dsh web` output includes a deterministic redacted-format base64url token and accepts `--no-open`.
- [x] Add a regression assertion that the current code fails: a fresh running Project's local and tailnet DSH links include the emitted token.
- [x] Verify a fresh `DshRuntimeManager` and restarted HTTP daemon return the same authenticated link while the runtime is alive.
- [x] Verify DSH restart replaces the token, stop removes links and token state, and repeated launch does not create a second process or token.
- [x] Verify the guest command includes `--no-open` and preserves non-interactive execution.
- [x] Run the guarded Rust test suite and confirm the hosting DSH PID remains alive and unchanged.

## Evidence

DSH 0.1.2-rc.1 rejects unauthenticated requests with HTTP 401 and exchanges `/?token=...` for an authority-bound cookie. Current links are built without a query token in `src/api.rs:117-130`, and `src/runtime.rs:26` launches `dsh web` without `--no-open`.

## Answer

Resumed after a provider protocol failure. The previously claimed implementation and its tests were already present on disk in this session's working tree: `src/runtime.rs` already launched `dsh web --no-open`, captured the token from the `dsh web:` startup line into per-Project runtime files (`/tmp/devvm-daemon-dsh.token` in the guest, `<log_dir>/<project-id>/dsh.token` on the host), and `tests/api_test.rs` already asserted token-carrying local, tailnet, and compatibility links plus the `tests/common/mod.rs` mock emitting a deterministic redacted base64url token. This ticket's lifecycle-token and `--no-open` scope was finished with targeted changes, and the hosting DSH PID stayed 1926, alive, with an unchanged pid-file mtime (1788588598) across every suite run (guard script: `.scratch/dsh-0.1.2-rc.1-upgrade/guard-pid-check.sh`).

What changed (all files were already modified before this session, so the diff below describes the net working-tree state against the ticket's stated baseline):

- `src/runtime.rs`:
  - `DSH_START_COMMAND` launches `exec dsh web --no-open`, and its prefixer loop captures `token="${line##*token=}"` / `token="${token%%[ &]*}"` from `*"dsh web:"*"token="*` lines, writing the token atomically (tmp + `mv`) to both the guest `/tmp/devvm-daemon-dsh.token` and the host `<log_dir>/<project-id>/dsh.token`. Stale pid and token state is removed only after the live-pid short-circuit and before the launch, so an idempotent relaunch never wipes the running DSH's token; `DSH_STOP_COMMAND` removes pid, guest token, and host token together.
  - `DshRuntimeManager` gained `get_token` (reads the host runtime file, trims, rejects blank) and `remove_token`; `get_status` drops the host token copy when a guest probe finds no live DSH, `launch_dsh`/`stop_dsh` no longer delete the host token (removal is owned by the guest snippet, which only runs on an actual launch or stop), and remaining host-side removals route through `remove_token`.
  - Unit tests assert the token capture pattern, the parse cuts, the atomic replacement, the live-check/stale-clear/launch ordering, and the stop cleanup.
- `tests/api_test.rs`:
  - `test_dsh_status_is_read_from_the_devvm_and_survives_a_daemon_restart` now asserts all three links contain the emitted token, that a fresh `DshRuntimeManager` returns the same token, that the host runtime file holds it, and that a restarted daemon serves the same authenticated links.
  - `test_second_launch_does_not_spawn_a_second_dsh_process` now asserts the host token copy survives a redundant launch.
  - `test_dsh_restart_replaces_the_running_process` now asserts the host token file holds the replaced token after restart.
  - `test_dsh_link_is_omitted_when_token_is_missing` now asserts a blank token file omits all three links while the runtime is running, and that a stopped runtime never presents an authenticated link, including after a simulated stale host token followed by a fresh probe.
  - New `test_project_logs_never_return_the_dsh_token_file` plants a token-bearing `dsh.log` and a `dsh.token` file and asserts the Project Logs API surfaces exactly the log entry and never the token file.
- `tests/common/mod.rs` (pre-existing): the mock `dsh web` accepts `--no-open`, emits `dsh web: http://127.0.0.1:3080/?token=mock-token-redacted-<count>-base64url-auth-token00`, and the mock `devvm` remaps the guest token path into the project's temp dir.
- `tests/live_acceptance_test.rs` (pre-existing, `#[ignore]`d): links assert the `?token=` prefix.
- `tests/acceptance_workflow_test.rs` (pre-existing): step 3 asserts the exact token-carrying local and tailnet links.
- `src/api.rs`, `src/logs.rs`, `src/models.rs` (pre-existing): link construction appends `?token=<token>` to `local_dsh_url`, `tailnet_dsh_url`, and the `dsh_url` alias; `logs::dsh_token_path`; the Project Logs reader only reads the three `*.log` files, never `dsh.token`.
- `.scratch/dsh-0.1.2-rc.1-upgrade/guard-pid-check.sh`: the hosting-PID guard used for the guarded suite runs.

Deviations:

- The net tree does not add the host-side `dsh_runtime_manager.remove_token` call before launch that the previous session's plan narrative mentioned; the guest snippet itself owns pre-launch stale-token removal, gated behind the live check, so the idempotent relaunch preserves the running token (required by "repeated launch does not create a second process or token").
- One planned regression assertion (token captured from the startup line into the host runtime file) is protected by the fresh-manager `get_token` equality plus the restart test, whose mock always emits a fresh token even for an unchanged startup, rather than by a capture line through `wait_for_dsh_log`.

Not verified here:

- The `tests/install_test.rs` link-invariant failure (`@devvm/dsh-remote-sync` etc. are still `file:` in `root/.dsh/profiles/web/package.json`) is ticket 04's scope, not this one.
- Real-browser token exchange (`/?token=...` → authority cookie under DSH 0.1.2-rc.1) and the Loopback Facade path on a live DevVM remain unexercised here (no live DevVM on this machine); the suite covers the mock-transport equivalents.
- `--trusted-domain` / `--trusted-host` were not added, and the localhost-subdomain patch and Caddy Loopback Facade were not touched — the requirement is met by leaving them alone.
