# AGENTS.md

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See `docs/agents/triage-labels.md`.

### Domain docs

This repository uses a single-context domain-doc layout. See `docs/agents/domain.md`.

### Runtime verification

Changes to DevVM or DSH Runtime lifecycle code must cover non-interactive execution, stop-and-relaunch behavior, DSH Status read back from the DevVM after a simulated daemon restart, and Project Log updates.

The DSH Runtime is started, stopped, and probed with three `devvm exec` snippets; the daemon holds no child process. Each Project has one host log directory, `<DEVVM_ROOT>/.project-logs/<project-id>/`, with `daemon.log` (daemon), `dsh.log` and `ingress.log` (guest), every line prefixed with an ISO-8601 UTC millisecond timestamp.

Lifecycle test fakes must isolate guest PID files and never evaluate commands against `/tmp/devvm-daemon-dsh.pid` on the host: the mock `devvm` rewrites the guest paths (`/tmp/devvm-daemon-dsh.pid`, `/devvm-root/.project-logs`, `/run/devvm`, `/root/workspace`, `/root/.dsh`) into the test's own directories before running the snippet with real `bash`. When changing that isolation, run the suite with a guard that verifies the hosting DSH PID remains alive and unchanged.

### Session Sync

The DSH plugin at `root/.dsh/plugins/remote-sync/` is the only Session Sync engine (ADR 0003); the daemon never runs rsync. Plugin tests (`node --test root/.dsh/plugins/remote-sync/test.mjs`) use real rsync over the local transport (no `ssh_host`). The web profile holds a pnpm copy of the plugin under `root/.dsh/profiles/web/node_modules/@devvm/dsh-remote-sync/`; after editing the plugin, refresh it with `DSH_HOME=/root/.dsh dsh plugin --profile web install --force`, or the boot test and the running DSH exercise stale code.
