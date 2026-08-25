# dev-vm

Per-project dev VMs for OpenCode coding agents, managed by smolvm.

## Image

- Python 3.14, Node 24, Rust (stable + clippy/rustfmt/rust-analyzer), `gh`, cargo-binstall
- OpenCode pre-installed with experimental LSP/scout/plan features enabled
- 16 CPUs / 8 GB RAM and networking
- Automatic HTTP access to development servers through FRP

## Files

| File | Purpose |
|---|---|
| `Dockerfile` | VM image |
| `build.sh` | Build image + export `rust-dev-opencode-<arch>.tar` |
| `devvm` | CLI wrapper around smolvm |
| `setup-devvm.sh` | Install smolvm, build image, link `devvm` |
| `smolvm.toml` | VM resources/auth |
| `opencode/` | OpenCode config (providers, agents, DCP) |

## Setup

```sh
./setup-devvm.sh
```

Requires Docker. One VM per project (named by path), mounting the project at `/workspace`.

## Usage

```sh
devvm shell          # open shell (creates VM if needed)
devvm start|stop|status
devvm exec <cmd>     # run one command
devvm rm             # delete VM
devvm name           # print machine name
```

Web servers are available immediately at a project-specific localhost name. A
server listening on port 3000 in the VM is available at:

```text
http://3000.<project-name>-<project-hash>.devvm.localhost
```

For example, `http://3000.my-project-5f32a810.devvm.localhost`. No port mapping
or VM restart is required. The server may listen on `127.0.0.1` inside the VM.
