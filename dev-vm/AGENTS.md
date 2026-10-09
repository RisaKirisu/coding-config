# AGENTS.md

## Agent skills

### Issue tracker

Issues and specs are tracked as local Markdown under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See `docs/agents/triage-labels.md`.

### Domain docs

This repository uses a single-context domain-doc layout. See `docs/agents/domain.md`.

### Runtime verification

`setup-devvm.sh` pins Smolvm to `1.19.3`; retain exact-version installation rather than a latest-release lookup.

Changes to DevVM or DSH Runtime lifecycle code must cover non-interactive execution, stop-and-relaunch behavior, DSH Status read back from the DevVM after a simulated daemon restart, and Project Log updates. DSH remains `Starting` while its guest PID is alive but its launch token is absent; `Running` requires the token captured from the current launch's ready URL, not merely a live process.

Lifecycle cancellation changes must pass `tests/lifecycle_test.rs` (real HTTP disconnects and process-group cleanup) and the two-client API checks. Per-Project coordination remains owned until interrupted commands are reaped; observed status is read independently of command ownership, without caching. See `readme.md` under Control Daemon & Web UI for cancellation and concurrency semantics.

The DSH Runtime is started, stopped, and probed with three `devvm exec` snippets; the daemon owns those commands until completion or cancellation cleanup, but holds no DSH Runtime child process. Each Project has one host log directory, `<DEVVM_ROOT>/.project-logs/<project-id>/`, with `daemon.log` (daemon), `dsh.log` and `ingress.log` (guest), every line prefixed with an ISO-8601 UTC millisecond timestamp.

Lifecycle test fakes must isolate guest PID files and never evaluate commands against `/tmp/devvm-daemon-dsh.pid` on the host: the mock `devvm` rewrites the guest paths (`/tmp/devvm-daemon-dsh.pid`, `/devvm-root/.project-logs`, `/run/devvm`, `/root/workspace`, `/root/.dsh`) into the test's own directories before running the snippet with real `bash`. When changing that isolation, verify that every rewritten path and child process belongs to the test fixture. Leave the runtime hosting the agent session untouched, including process probes.

### Project app

For the Control-origin app, embedded native DSH bridge, durable text, owner patch, or browser recovery checks, read `docs/project-app.md` for current contracts and proof boundaries. For native patch deployment, DSH release changes, Session-availability checks, or settings-persistence diagnosis, read `docs/dsh-runtime-maintenance.md` for public API boundaries, release inputs, and new/existing-VM procedures. Run real browser checks only against their owned `.agents` fixture. Use `tests/project_app_controls.cjs` for focused header, refresh, action, and drawer changes, and `tests/project_app_mobile.cjs` for mobile navigation and keyboard layout; desktop height emulation does not establish physical Android keyboard behavior.

### Control Daemon browser origin

For Control Daemon URLs, DSH launch links, browser authentication, or ingress changes, read ADR 0002. With DSH `0.2.0-rc.2`, use `http://control.devvm.localhost:8100` locally and `https://devvm.<remote-domain>` (default: `https://devvm.risak.dev`) over the tailnet: DSH exchanges its launch token for a `SameSite=Strict` cookie, so the Control and Project URLs must be same-site. Raw IP and bare `localhost` URLs remain management-only aliases because their Open DSH navigation is cross-site. Port `8100` reaches the Control Daemon directly locally (or via Host Caddy proxy remotely); Project URLs traverse FRP and Caddy.

The host FRP server appends stdout and stderr to `~/.local/state/devvm/frps.log`; existing server processes retain their original output destination until relaunched. Do not restart the shared server during read-only ingress diagnosis because that interrupts all Projects and clears proxy ownership evidence.

Host ingress has one configuration source: `scripts/Caddyfile.host`. Setup prints it; integration tests load it directly. Read `docs/remote-access.md` before changing setup or ingress. Report HTTP cookie-replay checks separately from browser HTTPS/SameSite verification.

### Session Sync

The DSH plugin at `root/.dsh/plugins/remote-sync/` is the only Session Sync engine (ADR 0003); the daemon never runs rsync. Plugin tests (`node --test root/.dsh/plugins/remote-sync/test.mjs`) use real rsync over the local transport (no `ssh_host`). Startup reconciliation runs from the installed Web-profile package so it uses the same plugin version as DSH.

### Build Loop

The DSH plugin at `root/.dsh/plugins/build-loop/` provides the `build_ticket` tool (build agent, then parallel review and test-audit agents, findings fed back to the same build agent for at most `maxFixRounds` rounds, all reports returned verbatim) and its **Settings → Build Loop** page. Prompts and flow live in the `build-loop` settings namespace; defaults in `prompts.mjs`. Child tool restrictions filter the configured denylist against global tool schemas inherited by children, safely ignoring parent-only or removed names. Audit children retry up to three launch attempts on failed or empty execution without consuming fix rounds; finished plain text falls back to clean:false findings, and exhausted audits fail with all latest reports preserved. Tests: `node --test root/.dsh/plugins/build-loop/test.mjs` (pure helpers only; no mocks). Orchestrators should dispatch ticket implementation through `build_ticket`, not a plain subagent.

### Updating a local DSH plugin

DSH image builds pin `0.2.0-rc.2`. Guest initialization in `devvm` runs native frozen-lockfile installs for Web/headless when installed locks differ. Plugin dependencies belong to package manifests and the profile lockfile; DSH supplies runtime peers through its native resolver. Keep `setup-devvm.sh` generic; DSH dependency installation belongs in the existing guest initialization path. MCP uses native HTTP headers; do not add a bearer-reference transport extension or an MCP source patch. Keep repository-wide DSH integration tests in `tests/`, not `root/.dsh/tests/`.

Local plugins are native `file:../../plugins/<dir>` dependencies in the Web profile. pnpm installs package contents and ordinary dependencies into the profile; DSH resolves runtime peers without a shared fallback link. Installed packages are snapshots, not live source links.

1. Run the plugin's tests. Verify installation changes with `node --test tests/dsh-plugin-install.test.mjs`, which installs an isolated profile with real DSH/pnpm and imports custom host plugins without source dependency directories or fallback links.
2. List new runtime modules in the plugin's `package.json` `files` and `exports` as needed.
3. Refresh source-only edits with `CI=true DSH_HOME=/root/.dsh dsh plugin --profile web install --force --frozen-lockfile --store-dir /root/workspace/.pnpm-store/v11`. For dependency or peer changes, omit `--frozen-lockfile` to regenerate the profile lockfile and retain that lockfile in the repository. An unchanged lockfile does not trigger installation during guest startup.
4. Let the user restart the DSH Runtime through the existing lifecycle. Package replacements require a fresh process; browser refresh alone does not reload them. Never restart the runtime hosting the current session.

Saved field overrides live in the active Runtime profile’s `cordis.patch.yml` and shadow plugin defaults. Reset through a profile-backed native form to remove an override. Published rc.2’s non-loopback native forms use memory-only state; their language/send changes do not persist, and supported config/Cordis APIs do not override that policy. Do not claim profile edits or restart fix it; keep the diagnostic probe and this limitation explicit during release admission.
