# 02: One configuration file and setup

**What to build:** The Control Daemon takes every setting from one `config.toml`, and a `setup` subcommand installs the pinned Smolvm and frps, creates or updates that file, and registers the user service. The bash `devvm` still drives DevVMs in this ticket; ticket 03 replaces it. Configuration schema and installation details are in the spec's Implementation Decisions.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] The daemon loads `config.toml` from the OS configuration directory (or the file named by a global `--config`): `root_dir`, `[server]` host/port/ingress port, `[remote]` domain, `[sync]`; unknown keys and missing required values fail with a clear error.
- [ ] Host-side `DEVVM_*` setting variables and the path flags (registry path, log directory, home directory, `devvm` binary path, service binary path, sync config path) are gone; modules receive the loaded configuration and tests construct it directly.
- [ ] `sync setup` writes `[sync]` and the guest-visible sync file through the same provisioning function as the HTTP API (kept writer ID, copied key, daemon URL); a running daemon shows the new Session Sync configuration without a restart.
- [ ] `setup` downloads the Smolvm 1.27.3 and frps 0.71.0 archives for the host platform, verifies them against the published SHA-256 checksums, and extracts them into the data directory; a checksum mismatch fails.
- [ ] `setup` checks Linux KVM access and that both DSH profile manifests exist under the given root directory, naming whatever is missing.
- [ ] `setup` creates `config.toml` (root directory required) or updates only the keys given, preserving comments and other keys.
- [ ] `setup` prints the host Caddy block and its environment settings when a remote domain is configured, and with `--service` registers, enables, and starts the user service, whose definition carries no settings.
- [ ] Tests assert the download, verify, and extract commands through the mocked command runner; checksum verification is tested on real files; no test uses the network or registers a real service.
- [ ] The readme and `AGENTS.md` describe the configuration file and `setup`.
