# dev-vm

DevVM Workspace Supervision: isolated development microVMs for OpenCode and DeepSeek Harness (DSH) coding agents, managed by [Smolvm](https://github.com/smol-machines/smolvm) with local and tailnet access, loopback facade ingress, and portable DSH state synchronization.

## System Architecture

- **Isolation**: One microVM per Project with CPUs/RAM configured from `smolvm.toml`.
- **Stable Working Directory**: Every Project is mounted at `/root/workspace` inside its microVM, providing a stable working directory across workstations.
- **DSH State Architecture**:
  - *Centrally Shared DSH Config*: Workstation-wide plugins, profiles, skills, presets, settings, and credentials reside on the host (`root/.dsh/`) and are mounted at `/devvm-root/.dsh`, linked into `/root/.dsh`.
  - *VM-Local Portable DSH State*: Project-specific conversation history (`/root/.dsh/sessions`), authoritative attachments (`/root/.dsh/attachments/v1/objects`), workspace data (`/root/.dsh/storages/workspace.json`), and message feedback (`/root/.dsh/storages/message_feedback.json`) remain in the DevVM filesystem and synchronize to a VPS Sync Store grouped by Project ID (`.devvm-id`). The rebuildable projection cache (`/root/.dsh/storages/session_projcache.json`) and workstation config are excluded.
- **Loopback Facade Ingress**: Ingress proxies (Caddy + FRP) rewrite incoming requests on local (`*.devvm.localhost`) and tailnet (`*.devvm.internal`) Project URLs to present loopback authority (`Host: localhost:<port>`, matching loopback `Origin`), avoiding per-application trusted-host config.
- **Network Boundary & Security**: The Control Daemon and Ingress bind strictly to local loopback (`127.0.0.1`) and the detected Tailscale IP address. No wildcard `0.0.0.0` binding or public LAN/Internet exposure is permitted. Access is secured entirely by the Tailnet Boundary without separate application logins.

## Files

| File / Directory | Purpose |
|---|---|
| `devvm` | CLI wrapper around Smolvm for shell and VM lifecycle |
| `devvm-daemon` / `src/` | Control Daemon, embedded Web UI, background runtime supervision, and wildcard DNS server |
| `scripts/devvm-ingress` | Ingress starter (Caddy + FRP client) streaming logs to host-persisted Project Logs |
| `scripts/Caddyfile` | Ingress Loopback Facade configuration |
| `scripts/frpc.toml` | FRP client virtual-host configuration |
| `setup-devvm.sh` | Complete version-one setup script for Linux and macOS |
| `root/` | Host-managed agent config mounted at `/devvm-root` |
| `skills/` | Central skill collection shared across all projects (shortcut to `root/skills/`) |
| `root/.dsh/plugins/remote-sync/` | DSH plugin for turn-triggered and manual Portable DSH State synchronization |

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

- **Locally**: Open `http://127.0.0.1:8100` (or `http://localhost:8100`).
- **Over Tailscale**: Open `http://<tailscale-ip>:8100`.

The Web UI allows you to:
1. Browse directories beneath `$HOME` and register Projects (creating or reading `.devvm-id`).
2. Start, stop, and delete DevVM instances.
3. Launch and monitor DSH Runtimes with direct browser links. DevVM and DSH actions show animated starting/stopping states; DSH links appear only after runtime readiness is confirmed.
4. Open arbitrary guest HTTP ports with instant Loopback Facade links.
5. Inspect host-persisted Project Logs (surviving VM stop/deletion).
6. View Sync Status (synchronized, synchronizing, degraded, or failed) and trigger manual syncs.

## Wildcard Private DNS (Tailscale)

To resolve `*.<project-host>.devvm.internal` from any device on your Tailnet:

```sh
# Generate split DNS setup instructions
devvm-daemon dns setup

# Run DNS server (default on port 53 or custom port)
devvm-daemon dns --bind 0.0.0.0:53
```

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
- **Tailnet URL**: `http://<PORT>.<project-name>-<project-hash>.devvm.internal:8102`

For example: `http://3080.my-app-5f32a810.devvm.localhost:8102`. All proxied traffic receives the Loopback Facade.
