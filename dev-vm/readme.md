# dev-vm

Per-project dev VMs for OpenCode coding agents, managed by [Smolvm](https://github.com/smol-machines/smolvm).

## Image

- Python 3.14, Node 24 + pnpm + NVM, Rust (stable + clippy/rustfmt/rust-analyzer), `gh`, cargo-binstall
- OpenCode pre-installed with experimental LSP/scout/plan features enabled
- CPUs/RAM from `smolvm.toml` and networking
- Automatic HTTP access to development servers through FRP; Caddy and FRPC are checked and started after every VM boot
- DeepSeek Harness patched to treat `*.localhost` as loopback, gated by hashes of the upstream bundles

## Files

| File | Purpose |
|---|---|
| `Dockerfile` | VM image |
| `build.sh` | Build image + export `rust-dev-opencode-<arch>.tar` |
| `devvm` | CLI wrapper around smolvm |
| `setup-devvm.sh` | Install smolvm, build image, link `devvm` |
| `smolvm.toml.example` | Template for VM resources/auth; `setup-devvm.sh` copies it to the gitignored `smolvm.toml` so `cpus`/`memory` can be tuned per host |
| `scripts/` | Caddy/FRPC config and VM ingress launcher |
| `root/` | Host-managed agent config and data mounted at `/devvm-root` |
| `skills/` | Central skill collection shared by every coding agent (host shortcut to `root/skills/`) |
| `root/.config/opencode/` | OpenCode config (providers, agents, DCP) |
| `root/.dsh/` | Shared DeepSeek Harness plugins and data |

## Setup

```sh
./setup-devvm.sh
```

Requires Docker or Podman. Docker is preferred when both are installed. On macOS also install `e2fsprogs` (`brew install e2fsprogs`) for `mkfs.ext4`; `setup-devvm.sh` aborts without it. On macOS the builder VM needs at least 8 GB RAM (the DeepSeek Harness install asks Node for an 8 GB heap), e.g. `podman machine set --memory 8192 --cpus 6`. One VM per project (named by path), with two host mounts:

- The project at `/root/<project-name>`.
- `root/` at `/devvm-root`. After each start, `devvm` links every entry into `/root`, replacing an existing entry with the same name. Children of `root/.config/` are linked individually into `/root/.config/`, so unmanaged config remains available. Adding another config or agent directory does not require rebuilding the image. In particular, `root/.dsh/` is linked to `/root/.dsh` in every VM.

## Shared skills

Put skill folders under `skills/`. OpenCode and DSH both link to this directory, so each skill only needs to exist once and is available in every Dev VM without rebuilding the image.

**Known Issue**: under x86_64 libkrun's virtio device count limit, at most 2 fs locations can be mounted. Any more will result in smolvm machine start error. Keeping the shared files under one root keeps the aggregate virtio device count when using the virtio-net backend.

## Usage

```sh
devvm shell          # open shell (creates VM if needed)
devvm start|stop|status
devvm exec <cmd>     # run one command
devvm rm             # delete VM
devvm name           # print machine name
```

Web servers are available immediately at a project-specific localhost name. A server listening on port 3000 in the VM is available at:

```text
http://3000.<project-name>-<project-hash>.devvm.localhost:8102
```

For example, `http://3000.my-project-5f32a810.devvm.localhost:8102`. No port mapping or VM restart is required. The server may listen on `127.0.0.1` inside the VM.
