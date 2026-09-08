# dev-vm

DevVM Workspace Supervision: isolated development microVMs for OpenCode and DeepSeek Harness (DSH) coding agents, managed by [Smolvm](https://github.com/smol-machines/smolvm) with local and tailnet access, loopback facade ingress, and portable DSH state synchronization.

## System Architecture

- **Isolation**: One microVM per Project with CPUs/RAM configured from `smolvm.toml`.
- **Stable Working Directory**: Every Project is mounted at `/root/workspace` inside its microVM, providing a stable working directory across workstations.
- **DSH State Architecture**:
  - *Centrally Shared DSH Config*: Workstation-wide plugins, profiles, skills, presets, settings, and credentials reside on the host (`root/.dsh/`) and are mounted at `/devvm-root/.dsh`, linked into `/root/.dsh`.
  - *VM-Local Portable DSH State*: Project-specific conversation history (`/root/.dsh/sessions`), authoritative attachments (`/root/.dsh/attachments/v1/objects`), per-session projection documents (`/root/.dsh/storages/session_projcache/sessions`), workspace data (`/root/.dsh/storages/workspace.json`), and message feedback (`/root/.dsh/storages/message_feedback.json`) remain in the DevVM filesystem and synchronize to a VPS Sync Store grouped by Project ID (`.devvm-id`). Derived request images, obsolete projection cache indexes, and workstation config are excluded. Projection documents are whole-record checkpoints that transfer newest-wins in their own pass, never through the append-only session union, so a stale receiver is replaced as a whole file instead of being corrupted by `--append-verify`.
- **Loopback Facade Ingress**: Ingress proxies (Caddy + FRP) rewrite incoming requests on local (`*.devvm.localhost`) and remote HTTPS (`<project-host>-<port>.<remote-domain>`) Project URLs to present loopback authority (`Host: localhost:<port>`, matching loopback `Origin`), avoiding per-application trusted-host config.
- **Network Boundary & Security**: The Control Daemon and Ingress bind strictly to local loopback (`127.0.0.1`). Remote access is opt-in and terminates HTTPS on the configured Tailscale IP via host-side Caddy. No wildcard `0.0.0.0` binding or public exposure is permitted. Access is secured entirely by the Tailnet Boundary without separate application logins.

## Files

| File / Directory | Purpose |
|---|---|
| `devvm` | CLI wrapper around Smolvm for shell and VM lifecycle |
| `devvm-daemon` / `src/` | Control Daemon, embedded Web UI, and background runtime supervision |
| `scripts/devvm-ingress` | Ingress starter (Caddy + FRP client) writing timestamped `ingress.log` into the Project's host log directory |
| `scripts/devvm-sync-startup` | Guest startup script that runs Session Sync reconciliation before `dsh web`; never blocks the launch |
| `scripts/Caddyfile` | Ingress Loopback Facade configuration |
| `scripts/frpc.toml` | FRP client virtual-host configuration |
| `setup-devvm.sh` | Complete version-one setup script for Linux and macOS |
| `root/` | Host-managed agent config mounted at `/devvm-root` |
| `skills/` | Central skill collection shared across all projects (shortcut to `root/skills/`) |
| `root/.dsh/plugins/remote-sync/` | DSH plugin that is the single Session Sync engine: automatic pushes after saved changes, startup reconciliation, status indicator, and manual retry |
| `root/.dsh/plugins/build-loop/` | DSH plugin providing the `build_ticket` tool (build → review ‖ test → fix loop) and its Settings → Build Loop configuration page |

## Setup

```sh
./setup-devvm.sh
```

Flags:
- `--service`: Automatically install and enable user service (`systemd --user` on Linux, `launchd` on macOS).
- `--skip-image`: Skip building the microVM tarball if already built.

### Prerequisites
- Docker or Podman (Docker preferred).
- On macOS: `brew install e2fsprogs` for `mkfs.ext4`, and allocate at least 8 GB RAM to the builder machine.
- Tailscale (optional, for remote private tailnet access).

## Control Daemon & Web UI

Run the Control Daemon interactively:

```sh
devvm-daemon serve
```

Or install as a persistent user service:

```sh
# Linux (systemd user service) / macOS (launchd agent)
devvm-daemon service install --enable --start
devvm-daemon service status
```

### Accessing the Web UI

Use a named Control Daemon URL that shares the Project URL's parent site:

- **Locally**: Open `http://control.devvm.localhost:8100`.
- **Over Tailscale**: Open `https://devvm.risak.dev` (or your configured `--remote-domain`).

`control.devvm.localhost` is not a Caddy route or an explicit daemon hostname. Browsers and modern resolvers map every `.localhost` name to loopback, and the Control Daemon's `127.0.0.1:8100` listener accepts that Host header. Remote access routes through host-side Caddy, which terminates HTTPS on the configured Tailscale IP and proxies to the loopback listeners.

The named URL is required for one-click DSH launch links under DSH `0.1.2-rc.1`. That release exchanges `/?token=<token>` for an `HttpOnly; SameSite=Strict` authority cookie, then redirects to `/`. A Control page opened at `127.0.0.1:8100`, `localhost:8100`, or a raw IP is cross-site relative to `*.devvm.localhost` or `*.risak.dev`; browsers store the cookie but withhold it on the redirect, and DSH responds with `dsh web authentication required`. The corresponding named Control and Project URLs share their parent domain and are same-site, so the redirect carries the cookie and succeeds.

The raw-IP and bare-localhost Control URLs remain usable for daemon management, but their Open DSH links have this DSH `0.1.2-rc.1` browser-auth limitation.

The Web UI allows you to:
1. Browse directories beneath `$HOME` and register Projects (creating or reading `.devvm-id`).
2. Start, stop, and delete DevVM instances.
3. Launch and monitor DSH Runtimes with direct browser links. DSH runs detached inside its DevVM and survives Control Daemon restarts. Each status request reads the VM and guest state without a daemon status cache. `running` requires a live PID and the current launch's ready-URL token; a live PID without that token is `starting`. A crashed DSH reads back as `stopped`, with the reason in its `dsh.log`.
4. Open arbitrary guest HTTP ports with instant Loopback Facade links.
5. Inspect host-persisted Project Logs (surviving VM stop/deletion). Each Project has one host log directory, `root/.project-logs/<project-id>/` (override with `DEVVM_LOG_DIR`), holding `daemon.log` written by the Control Daemon, plus `dsh.log` and `ingress.log` written inside the DevVM, which sees the directory at `/devvm-root/.project-logs/<project-id>/`. Every line starts with an ISO-8601 UTC timestamp in milliseconds.
6. View Sync Status (not configured, synchronizing, synchronized, remote ahead, degraded, or failed) as reported by the DSH Runtime, and restart a running DSH Runtime to pull work another workstation pushed. Manual retry lives in the DSH sync indicator, not here.

Lifecycle requests stay open until their commands finish; there is no execution timeout. Commands are coordinated per Project across all UI clients: conflicting starts/restarts return HTTP 409, while Stop VM, Delete VM, or Stop DSH cancels an active start and waits for command cleanup before proceeding. A client disconnect observed by the daemon cancels that client's unfinished operation. Cancellation sends SIGTERM to the host command's process group and waits for exit and captured-output closure before releasing the Project. It does not undo a VM that already started or stop a DSH Runtime after a successful detached launch. A command that ignores SIGTERM continues to hold the Project until it exits; no forced-cleanup deadline is imposed. When using a reverse proxy, disconnect cancellation depends on that proxy closing the upstream request.

The initiating UI shows its local pending action while the request runs; other UIs show observed VM/DSH status. The VM can therefore show Running while its startup command is still configuring the guest. Unregistering changes only the registry and does not cancel a live operation.

The log viewer merges the three files into one time-ordered list. Each row shows the local time (`HH:MM:SS.mmm`), a badge for its source (`daemon`, `dsh`, or `ingress`), and the message, with error rows tinted red and warnings amber; Caddy's JSON access lines are compacted to `GET /path → 502 (0.2 ms)`. The toolbar carries one chip per source, an `errors only` toggle, and a `Follow` toggle that keeps the view pinned to the newest line — scrolling up more than 24 px turns Follow off so the refresh every two seconds no longer moves the view, and scrolling back to the bottom turns it on again.

## Remote access

Remote access uses Cloudflare DNS-only records and host-side Caddy HTTPS termination over Tailscale. Remote setup is opt-in:
```sh
# Local-only (default)
./setup-devvm.sh --service

# Main workstation (defaults to risak.dev and 100.67.154.69)
./setup-devvm.sh --service --remote

# Custom overrides
./setup-devvm.sh --service --remote --remote-domain risak.dev --remote-ip 100.67.154.69
```

Setup prints the single [host Caddy site block](scripts/Caddyfile.host) and its environment settings; it does not write or overwrite a Caddyfile. Add that block to your host Caddy configuration. See [the remote-access architecture and host setup guide](docs/remote-access.md). Obsolete private DNS (`devvm-daemon-dns.service`, `devvm.internal`) is removed.

## CLI Usage

You can also use the standalone `devvm` CLI directly from any project directory:

```sh
devvm shell          # open interactive shell in microVM
devvm start|stop     # start or stop the microVM
devvm status         # check microVM status
devvm exec <cmd>     # run command in microVM (mounts project at /root/workspace)
devvm rm             # delete microVM
devvm name           # print machine name
```

### Project URLs

Servers listening inside the microVM on port `PORT` are accessible at:
- **Local URL**: `http://<PORT>.<project-name>-<project-hash>.devvm.localhost:8102`
- **Remote HTTPS URL**: `https://<project-name>-<project-hash>-<PORT>.<remote-domain>` (when `--remote` is enabled)

For example: `http://3080.my-app-5f32a810.devvm.localhost:8102` locally, or `https://my-app-5f32a810-3080.risak.dev` remotely. If the project name exceeds 48 characters, the project host is just its existing eight-character path hash in both local and remote URLs. All proxied traffic receives the Loopback Facade.
