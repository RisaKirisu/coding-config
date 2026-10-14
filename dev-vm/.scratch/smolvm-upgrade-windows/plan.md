# Plan: Upgrade Smolvm to 1.27.3 and support native Windows hosts

Status: superseded by `.scratch/devvm-refactor/plan.md`

## Goal

1. Run every DevVM on Smolvm 1.27.3 on Linux and macOS.
2. Run the Control Daemon, `devvm`, host ingress (frps), and DevVMs natively on Windows x86_64 hosts using Smolvm's Windows Hypervisor Platform (WHP) backend, with the same guest image and the same Project, DSH Runtime, Project Log, Session Sync, and Project URL behavior as on Linux and macOS.

Windows on arm64 is out of scope because Smolvm ships no Windows arm64 build. WSL2 is not used.

## Decisions

| Decision | Choice | Reason |
|---|---|---|
| Smolvm version | Pin `1.27.3`. At implementation time, pin a newer release instead if it contains the fix for the virtio-net relay CPU spin ([PR #1740](https://github.com/smol-machines/smolvm/pull/1740)) and passes the same acceptance checks. | 1.27.3 is the newest release (2026-10-13). #1740 is the main open regression for this workload. |
| Existing DevVMs | Delete every DevVM and all Smolvm 1.19.3 state, then recreate the DevVMs under 1.27.3. No code keeps old machines, old machine names, or old guest layouts working. | A clean reinstall removes all migration and compatibility code. Portable DSH State survives through the Sync Store. |
| `devvm` implementation | Replace the bash `devvm` script with a Rust `devvm` binary in this crate. Use it on all three OSes. The commands the daemon calls keep their arguments and output formats. | Bash cannot run natively on Windows, and a second PowerShell implementation would duplicate the same logic. Keeping the daemon-facing commands unchanged leaves the daemon and its mockall tests untouched. |
| Project identity | `devvm` and the daemon share one rule: the existing `compute_project_host` logic in `src/models.rs:172-209`. The machine name is `dev-<name>-<hash>` from the same name and hash. | One rule for both programs. The bash script's slightly different name rule (it also squeezes existing repeated `-`) is dropped; machine names may change, which the clean reinstall absorbs. |
| Windows setup | Add `setup-devvm.ps1`. Keep `setup-devvm.sh` for Linux and macOS. | Setup builds the Rust binaries, so it must be a native script per OS. |
| Windows user service | `devvm-daemon service install` registers a per-user Scheduled Task that runs `devvm-daemon serve` at logon. | Smolvm state lives in the user's `%LOCALAPPDATA%\smolvm`, so the daemon must run as that user. A per-user Scheduled Task needs no admin rights. |
| Windows memory return | `devvm` sets `SMOLVM_WHP_RECLAIM=discard` on every `smolvm` invocation on Windows. | By default WHP keeps freed guest pages resident until host free RAM drops to 25%, except during Smolvm's idle-reclaim pulses. The cache-reclaim loop expects prompt return. The variable reaches the `_boot-vm` process through normal environment inheritance (`src/agent/manager.rs:2866-2997`), and the bundled libkrun accepts `discard` and `keep`. |
| Windows host prerequisites | Windows 11, or Windows 10 1809 or later, x86_64; `HypervisorPlatform` feature enabled; Developer Mode enabled; Docker Desktop with Linux containers; Git with `core.symlinks=true`. | Smolvm's networking needs native AF_UNIX (Windows 10 1809+). Smolvm extracts its agent rootfs with symlinks and fails without symlink privilege (`src/agent/manager.rs:1308-1320`). The repo tracks two symlinks (`root/.dsh/skills`, `root/.config/opencode/skills`) that the guest needs. Developer Mode grants symlink creation to the normal user for both. Smolvm's own Windows testing covers Windows 11 Home; Windows 10 is a lower bound, not a tested guarantee. |

## Phase 1: Smolvm 1.27.3 and Rust `devvm` on Linux and macOS

### 1.1 Setup

1. `setup-devvm.sh:5`: `SMOLVM_VERSION=1.27.3`.
2. `setup-devvm.sh:37-44`: `installed_smolvm_version` reads only `smolvm --version` (output `smolvm 1.27.3`) and no longer reads `~/.smolvm/.version`.
3. `setup-devvm.sh` installs the built `devvm` binary into `~/.local/bin` beside `devvm-daemon` and no longer symlinks the script.
4. `tests/install_test.rs`: update the pinned versions (`:724`, `:744`) and replace the version-file case (`:755-763`) with a test of the `smolvm --version` parser on that output. No fake executable on `PATH` (AGENTS.md:27). Update the upstream source references (`:477-478`, `:608`) to `v1.27.3`.

No changes to `smolvm.toml`, `scripts/frpc.toml`, or the Smolvm arguments: create/start/stop/delete/exec flags, `machine ls --json` fields (`name`, `state`, `memory_mib`, `pid`), exit-code propagation, the exec's inherited working directory and environment, survival of redirected background loops after `exec` returns, the `100.96.0.1` gateway to host loopback, `/storage`, virtiofs shares, writable cgroup v2, and free page reporting are the same in 1.27.3.

### 1.2 Rust `devvm`

Add a binary target `devvm` (`src/bin/devvm.rs`) and a library module `src/devvm.rs`; delete the bash `devvm`.

Commands:

- `start`, `shell` (also the default with no arguments), `exec`, `stop`, `status`, `memory`, `rm`, `name`; any other argument prints the usage line and exits 1.
- `exec` accepts the guest command with or without a leading `--`, passes stdio through, returns the guest exit code, and does not create or start the machine.
- `start` and `shell` create the machine if absent and start it if not running; the guest setup steps (ingress, `link-root`, `/storage/tmp` cleanup with `fstrim`, cache-reclaim loop) run only when a start actually happens.
- `status` prints nothing for an absent machine and `Machine '<name>': <state>` otherwise (`src/runner.rs:195-211` parses it).
- `memory` prints `{"host_bytes":…,"guest_bytes":…,"limit_bytes":…}` with `null` for unavailable values and exits 1 when the machine is not running; `guest_bytes` comes from the guest's `/proc/meminfo` (`MemTotal - MemAvailable`) (`src/runner.rs:214-237` parses it).
- A non-empty `.devvm-id` is kept; otherwise a new UUID v4 is written. The Project directory is the working directory.

Implementation:

| Operation | Rust |
|---|---|
| Project name, hash, host, machine name | Split `compute_project_host` (`src/models.rs:172-209`) into name, hash, and host functions used by both the daemon and `devvm`; the machine name is `dev-<name>-<hash>`. |
| Smolvm arguments | Functions returning the argument vectors, unit-tested with the expectations `tests/install_test.rs` asserts today. |
| Machine listing | Parse `machine ls --json` with `serde_json` into `name`, `state`, `memory_mib`, `pid`. |
| Host ingress | Reuse an frps process owned by the current user whose executable and arguments equal the expected command (`sysinfo`); otherwise spawn frps detached with output appended to `~/.local/state/devvm/frps.log` (Windows: `%LOCALAPPDATA%\devvm\frps.log`). |
| Host memory of the VM | `sysinfo` resident memory (Windows: working set) of the `pid` from `machine ls --json`, which is the VM process. |
| Guest snippets (`link-root`, `cache-reclaim`, `/storage` cleanup) | Move to `scripts/guest/*.sh`, embed with `include_str!`, run with `machine exec -- /bin/bash -c <snippet> <label>`. In `link-root`, delete the migration of old `/root/.dsh/{sessions,storages,attachments}` contents and the removal of an old `/root/.dsh` symlink (`devvm:168-180`); a new DevVM never has either. Also remove the skips when `dsh` is missing or a profile has no `package.json` (`devvm:195-200`): the image always installs DSH (`Dockerfile:168-171`) and both profile manifests are tracked, so a missing one fails the start. The lockfile staleness check stays. |
| `exec`, `shell` | Spawn `smolvm machine exec --stream` / `-it` with inherited stdio; exit with its code. |

Tests and documentation:

- Move the `test_devvm_*` cases in `tests/install_test.rs` to Rust unit tests in `src/devvm.rs`; remove only `devvm` from the loop in `test_scripts_can_be_sourced_without_actions` (`:461-472`); keep the `setup-devvm.sh` cases. Guest snippets are checked with `bash -n` on Unix hosts, as today (`:597`).
- `tests/observability_test.rs:125-130` copies the bash `devvm` to test a permission-denied start; point it at `env!("CARGO_BIN_EXE_devvm")`.
- `readme.md`: pinned version, Files table, CLI Usage, and the reinstall procedure below. `AGENTS.md`: pinned version and the paragraph about named functions in `devvm` and `setup-devvm.sh`.

### 1.3 Reinstall procedure (the user performs it)

1. In each Project, check that the Sync Status is `synchronized`, so the Sync Store holds the Project's Portable DSH State. A Project without a Sync Store loses its VM-local DSH sessions when its DevVM is deleted unless `/var/lib/devvm-dsh` is copied out of the guest first.
2. With the old `devvm`, run `devvm rm` in every Project.
3. Remove Smolvm 1.19.3 and all of its state: `~/.smolvm`; the `smolvm` directory under the data root; and the `smolvm`, `smolvm-image-archives`, `smolvm-pack`, and `smolvm-libs` directories under the cache root. The data root is `$XDG_DATA_HOME` (default `~/.local/share`) on Linux and `~/Library/Application Support` on macOS; the cache root is `$XDG_CACHE_HOME` (default `~/.cache`) on Linux and `~/Library/Caches` on macOS. On Linux, if `SMOLVM_DATA_DIR` was set, Smolvm used it as its home directory with the default Linux roots, so remove the same directories under it (`$SMOLVM_DATA_DIR/.local/share/smolvm`, `$SMOLVM_DATA_DIR/.cache/smolvm`, and so on).
4. Run `./setup-devvm.sh` (with `--service` if used).
5. Start each Project's DevVM and DSH Runtime from the Control Daemon; startup reconciliation pulls the Portable DSH State from the Sync Store.

### 1.4 Acceptance (Linux x86_64 and macOS arm64)

- `cargo test` passes.
- The opt-in `tests/live_acceptance_test.rs` passes.
- A new DevVM survives two stop/start cycles with its guest files intact.
- In the guest: both shares are mounted read-write; the `/root/.dsh` bind mounts exist; `/sys/fs/cgroup/memory.reclaim` is writable; `fstrim /storage` succeeds; the cache-reclaim loop is still running after the `devvm exec` that started it returned.
- `devvm exec -- false` exits 1; `devvm exec -- sh -c 'exit 3'` exits 3; `devvm shell` opens an interactive shell.
- Guest frpc connects to host frps through `100.96.0.1:17007`, and the daemon's Project URL serves through Caddy, including for a Project directory whose name contains `--`.
- After the reinstall, a Project's DSH sessions are restored from the Sync Store.
- `devvm memory` reports host bytes that fall after guest file cache is reclaimed.
- A DevVM with three mounts and virtio-net boots (x86_64 interrupt limit raised in v1.23.0).
- The Smolvm VM process does not stay at high CPU after DSH and frpc connections close (watch for #1740).

## Phase 2: Native Windows host

### 2.1 `devvm` on Windows

- `smolvm.exe` and `frps.exe` are found on `PATH`, installed by `setup-devvm.ps1`.
- Host paths are passed to `-v` in Windows form (`C:\Users\me\proj:/root/workspace`); Smolvm parses the guest path from the right (`src/data/storage.rs:163-180`). Projects are limited to directories beneath the user's home by the Project Browser.
- Every `smolvm` invocation gets `SMOLVM_WHP_RECLAIM=discard`.
- Captured output must return. A detached child that inherits the caller's output pipe keeps a captured `devvm start` waiting until that child exits; Smolvm's Windows notes record this hang for `machine start` (v1.22.2), and Rust on Windows spawns with handle inheritance enabled. At startup on Windows, `devvm` clears the inherit flag on its own standard handles (`SetHandleInformation`), runs `smolvm machine start` with stdout and stderr redirected to a file it then prints, and spawns frps with its log file as stdout and stderr. Inherited stdio for `exec` and `shell` is unaffected because Rust duplicates the handles it passes to a child.
- `devvm shell` needs a Windows console (Windows Terminal, PowerShell, or cmd). Smolvm's Windows interactive mode reads only console input events (`src/agent/terminal.rs:475-544`), so redirected stdin and its end of file are never forwarded and the session would hang. On Windows, `devvm shell` checks that stdin is a terminal and otherwise exits 1 with `devvm shell requires a console`; a test runs it with null stdin and expects that exit. The daemon never uses `shell`.
- `DEVVM_HOME` defaults to `%USERPROFILE%\coding-config\dev-vm`, matching the Unix default `$HOME/coding-config/dev-vm`.

### 2.2 Control Daemon on Windows

- Cancellation (`src/lifecycle.rs:306-375`): the Unix path stays as is (process group, SIGTERM, SIGKILL on drop), gated with `cfg(unix)`. The Windows path terminates `devvm` and its descendant processes found through `sysinfo` parent IDs, except the detached services it launched (`smolvm.exe _boot-vm …` and `frps.exe`), then waits for exit and captured-output closure. A Job Object is not used because the VM process would be inside the job. Behavior matches readme.md: a cancelled start does not undo a VM that already started, and the Project is released only after cleanup.
- Service management (`src/service.rs`): add `Platform::Windows`. The daemon executable defaults per platform: `~/.local/bin/devvm-daemon` on Unix (`src/service.rs:274-279`) and `%LOCALAPPDATA%\devvm\bin\devvm-daemon.exe` on Windows, where setup installs it; the task uses that absolute path, quoted because it may contain spaces. `install` registers the Scheduled Task `devvm-daemon` through `schtasks.exe` (`/SC ONLOGON`, current user, `devvm-daemon serve` with the same arguments as the systemd unit); `--start` runs it. `status` and `uninstall` query and delete the task through `schtasks.exe` instead of checking a unit-file path (`src/service.rs:400-404`, `:427-436`). `schtasks.exe` invocations go through a command boundary so tests assert the generated arguments; the existing test at `src/main.rs:444-475`, which installs for `Platform::current()` and asserts the generated unit or plist file, becomes a test of generated content per platform, so `cargo test` on Windows never registers a real task. Task-generation tests assert the executable path and its quoting.
- Session Sync setup (`src/main.rs:291-294`): default the config path to `DaemonConfig::new().sync_config_path` so `sync setup` writes where the daemon reads. The current hard-coded `~/.config/devvm/sync.json` differs from `dirs::config_dir()` on Windows (`%APPDATA%`) and on macOS (`~/Library/Application Support`). Add a test that the daemon reads back what setup wrote.
- Session Sync SSH on Windows: setup checks that the Windows OpenSSH client (`ssh.exe`) is on `PATH` before the connectivity check. Windows OpenSSH refuses a private key that other users can read, so setup stops with an `icacls` instruction when the source key's ACL is too broad, and restricts the copied key and the shared `.ssh` directory to the current user (the Windows counterpart of the `0700` mode at `src/sync.rs:125-131`).
- No other daemon change: home resolution already falls back to `dirs::home_dir()` (`src/config.rs:31`, `src/service.rs:100-101`), `DEVVM_ROOT` resolution and Project Logs use `PathBuf`, and the Project Browser canonicalizes both paths before its home check (`src/browser.rs:37-65`).

### 2.3 `setup-devvm.ps1`

1. Check x86_64; Windows 10 1809 or later; `HypervisorPlatform` enabled (otherwise print `Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform`, which needs admin and a reboot, and exit); Developer Mode enabled; `tar.exe`, `git`, `docker`, and `cargo` available; `git config core.symlinks` is `true` and both tracked symlinks are real symlinks in the checkout.
2. Install Smolvm 1.27.3 unless `smolvm --version` already reports it: download `smolvm-1.27.3-windows-x86_64.zip` and `checksums.sha256` from the release, verify the hash, and copy the contents of the zip's `smolvm-1.27.3-windows-x86_64\` directory to `%LOCALAPPDATA%\Programs\smolvm`, keeping all files together; add that directory to the user `PATH`.
3. Install frps 0.71.0 from `frp_0.71.0_windows_amd64.zip` to `%LOCALAPPDATA%\devvm\bin\frps.exe`.
4. Create `smolvm.toml` from the example if absent. Build the image with `docker build` and `docker save` to `rust-dev-opencode-amd64.tar`, with the same staleness check as `setup-devvm.sh`.
5. `cargo build --release`; copy `devvm.exe` and `devvm-daemon.exe` into `%LOCALAPPDATA%\devvm\bin` and add it to the user `PATH`.
6. `-Service`: run `devvm-daemon service install --enable --start`.
7. `-Remote`: print `scripts/Caddyfile.host` and its environment settings, as `setup-devvm.sh` does.

### 2.4 Repository

- Add `.gitattributes` forcing LF line endings for `Dockerfile`, `scripts/**`, `patches/**`, `root/**`, `*.sh`, and `*.toml`. A CRLF checkout would break guest scripts copied into the image and files mounted at `/devvm-root`.

### 2.5 Tests

- `tests/common/mod.rs:10,47-49` builds exit statuses with Unix `ExitStatusExt` (`code << 8`); add a Windows branch that uses the code directly, so the mockall suites compile and run on Windows.
- Gate Unix-only tests with `cfg(unix)`: `src/sync.rs:487-540` (file modes), `tests/observability_test.rs` permission test, `tests/lifecycle_test.rs` process-group tests, `tests/project_app_fixture.rs` (Linux browser fixture), the bash cases in `tests/install_test.rs`, and the bash probe test near `src/runtime.rs:470`. Add Windows tests for the Windows cancellation path in `tests/lifecycle_test.rs` using real child processes, and for Scheduled Task argument generation.
- `tests/live_acceptance_test.rs:24-48` and `tests/frp_config_test.rs:74-82` locate tools with `which` and Unix paths; use a cross-platform lookup (`which` crate) so they do not silently skip on Windows.
- `tests/live_acceptance_test.rs:346,383` requires host `rsync` but never runs it, because Session Sync runs in the guest plugin (AGENTS.md:47). Remove that prerequisite; the guest and Sync Store checks stay.
- The normal check set adds `cargo check --tests --target x86_64-pc-windows-gnu`, so Unix-only code cannot return unnoticed. Running `cargo test` and the acceptance list on a Windows host is done on the user's Windows machine because no Windows host is available to agents.

Windows acceptance (on a Windows 11 host): the Phase 1 acceptance list, except the reinstall, plus:

- A Project under `C:\Users\…` mounts at `/root/workspace`, and `/root/.dsh/skills` resolves to the shared skills in the guest.
- The daemon's captured `devvm start` returns, both when frps is already running and when it is not.
- Cancelling a start from the UI releases the Project, leaves frps and any started VM running, and a later start succeeds.
- The Scheduled Task starts the daemon at logon; `service status` and `uninstall` work.
- `devvm memory` host bytes fall after guest cache reclaim.
- In a Windows Project: `git status`, `pnpm install`, and `cargo build` in `/root/workspace` work over virtiofs on NTFS.
- `sync setup` succeeds and the Sync Status appears in the UI.

### 2.6 Documentation

- `readme.md`: Windows prerequisites, setup with `setup-devvm.ps1`, service management with Scheduled Tasks, and `devvm shell` needing a console.
- `docs/remote-access.md`: Windows host Caddy and Tailscale placement.
- `AGENTS.md`: the Windows check (`cargo check --tests --target x86_64-pc-windows-gnu`) and the Windows acceptance list.

## Risks

| Risk | Effect | Handling |
|---|---|---|
| [PR #1740](https://github.com/smol-machines/smolvm/pull/1740) open | The Smolvm process spins at 100% CPU after a half-closed then reset TCP connection through virtio-net. | Prefer a release with the fix; check CPU during acceptance. |
| [#992](https://github.com/smol-machines/smolvm/issues/992) | On Linux, 1.27.3 can create a new machine's default-sized overlay disk as qcow2 (`src/storage.rs:629-650`), and qcow2 files do not shrink on the host after trim. The 50 GiB storage disk that `fstrim /storage` trims stays raw, as do all disks on macOS and Windows. | Accept. |
| Windows virtiofs on NTFS | File modes, case-insensitivity, and hard links differ from ext4. | Covered by the Windows Project acceptance check. |
| Windows interrupt limit | The 19-slot x86_64 limit is not measured on WHP. | The three-mount acceptance check runs on Windows too. |
