# AGENTS.md

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See `docs/agents/triage-labels.md`.

### Domain docs

This repository uses a single-context domain-doc layout. See `docs/agents/domain.md`.

### Runtime verification

Changes to DevVM or DSH Runtime lifecycle code must cover non-interactive execution, stop-and-relaunch behavior, DSH Status read back from the DevVM after a simulated daemon restart, and Project Log updates. DSH remains `Starting` while its guest PID is alive but its launch token is absent; `Running` requires the token captured from the current launch's ready URL, not merely a live process.

Lifecycle cancellation changes must pass `tests/lifecycle_test.rs` (real HTTP disconnects and process-group cleanup) and the two-client API checks. Per-Project coordination remains owned until interrupted commands are reaped; observed status is read independently of command ownership, without caching. See `readme.md` under Control Daemon & Web UI for cancellation and concurrency semantics.

The DSH Runtime is started, stopped, and probed with three `devvm exec` snippets; the daemon owns those commands until completion or cancellation cleanup, but holds no DSH Runtime child process. Each Project has one host log directory, `<DEVVM_ROOT>/.project-logs/<project-id>/`, with `daemon.log` (daemon), `dsh.log` and `ingress.log` (guest), every line prefixed with an ISO-8601 UTC millisecond timestamp.

Lifecycle test fakes must isolate guest PID files and never evaluate commands against `/tmp/devvm-daemon-dsh.pid` on the host: the mock `devvm` rewrites the guest paths (`/tmp/devvm-daemon-dsh.pid`, `/devvm-root/.project-logs`, `/run/devvm`, `/root/workspace`, `/root/.dsh`) into the test's own directories before running the snippet with real `bash`. When changing that isolation, run the suite with a guard that verifies the hosting DSH PID remains alive and unchanged.

### Control Daemon browser origin

For Control Daemon URLs, DSH launch links, browser authentication, or ingress changes, read ADR 0002. With DSH `0.1.5-rc.2`, use `http://control.devvm.localhost:8100` locally and `https://devvm.<remote-domain>` (default: `https://devvm.risak.dev`) over the tailnet: DSH exchanges its launch token for a `SameSite=Strict` cookie, so the Control and Project URLs must be same-site. Raw IP and bare `localhost` URLs remain management-only aliases because their Open DSH navigation is cross-site. Port `8100` reaches the Control Daemon directly locally (or via Host Caddy proxy remotely); Project URLs traverse FRP and Caddy.

Host ingress has one configuration source: `scripts/Caddyfile.host`. Setup prints it; integration tests load it directly. Read `docs/remote-access.md` before changing setup or ingress. Report HTTP cookie-replay checks separately from browser HTTPS/SameSite verification.

### Session Sync

The DSH plugin at `root/.dsh/plugins/remote-sync/` is the only Session Sync engine (ADR 0003); the daemon never runs rsync. Plugin tests (`node --test root/.dsh/plugins/remote-sync/test.mjs`) use real rsync over the local transport (no `ssh_host`). First-party plugins in the web profile use pnpm `link:` dependencies, so `node_modules` resolves directly to `root/.dsh/plugins/`; never replace them with `file:` dependencies, whose hard-linked files can become stale after atomic source-file replacement.

### Build Loop

The DSH plugin at `root/.dsh/plugins/build-loop/` provides the `build_ticket` tool (build agent, then parallel review and test-audit agents, findings fed back to the same build agent for at most `maxFixRounds` rounds, all reports returned verbatim) and its **Settings → Build Loop** page. Prompts and flow live in the `build-loop` settings namespace; defaults in `prompts.mjs`. Child tool restrictions filter the configured denylist against global tool schemas inherited by children, safely ignoring parent-only or removed names. Audit children retry up to three launch attempts on failed or empty execution without consuming fix rounds; finished plain text falls back to clean:false findings, and exhausted audits fail with all latest reports preserved. Tests: `node --test root/.dsh/plugins/build-loop/test.mjs` (pure helpers only; no mocks). Orchestrators should dispatch ticket implementation through `build_ticket`, not a plain subagent.

### Updating a local DSH plugin

Local plugins (`root/.dsh/plugins/*`, mounted in the DevVM at `/root/.dsh/plugins/*`) are installed into the web profile as symbolic links via pnpm `link:` specifications (`~/.dsh/profiles/web/node_modules/<pkg> -> /root/.dsh/plugins/<pkg>`), with plugin peer dependencies resolving through the `$DSH_HOME/plugins/node_modules -> ../profiles/node_modules` fallback link. Source edits are therefore immediately visible at the installed path. After editing plugin source, inside the DevVM:

1. Run the plugin's own tests (`npm test` or `node --test ...` under `/root/.dsh/plugins/<dir>/`).
2. If you added a module, list it in the plugin's `package.json` `files` and `exports`.
3. If dependencies or peer dependencies change, re-run `cd ~/.dsh/profiles/web && CI=true DSH_HOME=/root/.dsh dsh plugin --profile web install --store-dir /root/workspace/.pnpm-store/v11`.
4. Restart the DSH Runtime (`devvm` stop/start). Host bundles are imported once; there is no HMR for them, and `client.js` is served from the profile copy, so a browser refresh alone is not enough.

Plugin defaults that are also exposed as settings (for example `build-loop` personas) are shadowed by any value saved in `~/.dsh/settings.yaml`; reset the field from its settings page to pick up a new default.
