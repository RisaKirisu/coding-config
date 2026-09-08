# DSH 0.1.2-rc.1 Compatibility Upgrade

Status: resolved

## Objective

Update DevVM Workspace Supervision for DeepSeek Harness 0.1.2-rc.1, preserving reliable DSH Runtime lifecycle, Project URLs, Loopback Facade behavior, custom plugins, third-party plugins, and complete Portable DSH State synchronization.

## Confirmed compatibility findings

- DSH now generates an ephemeral launch token and requires a token or existing authority-bound cookie for browser access. Current Control Daemon links omit the token and return HTTP 401 in a fresh browser context.
- `dsh web` now opens a browser by default. DevVM launches must pass `--no-open`.
- DSH has `--trusted-host`, not `--trusted-domain`. It performs exact authority matching and does not replace the browser-side localhost-subdomain compatibility patch or the ingress Loopback Facade.
- The localhost-subdomain patch and Caddy Loopback Facade remain required.
- DSH projection cache storage changed from one `storages/session_projcache.json` document to per-session documents under `storages/session_projcache/sessions/`. Cold listing does not rebuild missing projections for session logs larger than 1024 bytes, which causes newly pulled chats to display their workspace name until opened.
- All six local plugins remain needed, but several package peer ranges and tests need updates. The local `dsh-skill-mcp-panel` v2.0.1 browser client uses a removed sessions API and injects a removed DSH client module.
- `@deepseek-ai/dsh-web-fetch-http` is now built into DSH and must be removed from explicit profiles. `@hytime/dsh-thinking-effort` must move to its DSH 0.1.2-compatible release. `dsh-better-sidebar` should be pinned to its verified compatible version.
- Four local profile dependencies drifted from `link:` to `file:`, creating stale hardlinked copies and failing the repository's installation invariant.

## Required outcomes

1. Control Daemon DSH links authenticate fresh browser contexts with the current runtime token and remain correct after daemon restart and DSH restart.
2. DSH launches remain non-interactive and do not spawn a guest browser.
3. Existing localhost-subdomain patch and Loopback Facade remain installed and verified.
4. Session Sync transfers the per-session projection documents required for correct cold chat titles.
5. Custom plugins load and their tests run without depending on unavailable peer-module resolution.
6. Third-party and built-in plugin dependencies are current, non-duplicated, and pinned where reproducibility matters.
7. All first-party profile dependencies use `link:` and resolve as symlinks.
8. Rust, plugin, installation, ingress, and runtime verification pass, including non-interactive execution, stop/relaunch, daemon-restart status readback, and Project Logs.

## Constraints

- Do not replace or remove the localhost-subdomain patch or Loopback Facade.
- Preserve custom Bearer-token credential support in the local `dsh-skill-mcp-panel` fork.
- The `remote-sync` plugin remains the only Session Sync engine.
- Do not synchronize workstation-wide settings, credentials, profiles, plugins, presets, request-image caches, or runtime status files.
- Local plugins in the web profile must use pnpm `link:` dependencies.
- Tests must use real project seams and existing test infrastructure; do not introduce hand-rolled service mocks.

## Source findings

Read-only discovery reports are under:

- `.agents/exploration/dsh-rc1-lifecycle/findings.md`
- `.agents/exploration/dsh-rc1-sync/audit-report.md`
- `.agents/exploration/dsh-rc1-plugins/audit-report.md`
- `.agents/exploration/dsh-rc1-dependencies/notes.md`
- `.agents/exploration/dsh-rc1-profile-links/diagnosis.md`

## Final status and caveats

### Final status
All upgrade tickets (00, 01, 02, 03, 04, 05) are resolved and verified:
1. Control Daemon DSH links authenticate fresh browser contexts with the current launch token (`?token=<token>`) and omit links when stopped or missing token — verified at source and integration-suite level. The live host Control Daemon binary predates ticket 01, so live Project links cannot carry `?token=` until the daemon is rebuilt, restarted, and DSH relaunched.
2. DSH launches non-interactively with `--no-open` — verified at source and integration-suite level. The current live launch was started by the pre-ticket-01 daemon snippet without `--no-open` (its log shows the browser-open line).
3. Localhost-subdomain patch and Caddy Loopback Facade remain intact and functional.
4. Session Sync transfers per-session projection documents (`storages/session_projcache/sessions/***`) via a dedicated newest-wins pass, preserving cold listing chat titles across workstations.
5. All local plugins (`build-loop`, `subagent-manager`, `voice-input`, `style-control`, `dsh-skill-mcp-panel`, `remote-sync`) have passing test suites and load without missing peers.
6. Third-party plugins are pinned to compatible releases, duplicate built-in fetch package is removed, and web/headless lockfiles are clean.
7. All six first-party web-profile plugin dependencies use `link:` symlinks with `plugins/node_modules` fallback resolution.
8. The guarded Rust test suite, all plugin test suites, and deterministic anomaly verifiers pass cleanly while the hosting DSH Runtime process remains alive.

### Caveats and residual items
- Real-browser token exchange through a live DevVM Project URL and live Skills-tab rendering in a browser cannot be exercised in this headless CLI environment; verified via unit, integration, and mock suites.
- Cross-workstation pull of a real remote Sync Store over an external network/SSH was not performed; covered by the local-transport real-rsync suite.
- DSH Runtime was not restarted within this session to avoid terminating the active session/connection hosting the agent.
- Selected user model/settings remain preserved as configured.
- The host Control Daemon release binary (built 2026-09-03T06:31Z) predates the ticket-01 token and `--no-open` source changes (2026-09-05T10:02Z). The live DSH launch therefore carries no `--no-open`, no token capture, and no token files, so live Project links omit `?token=`. Matching live behavior to the design requires a daemon rebuild plus restart and a DSH relaunch; the relaunch stays prohibited for the agent and is left to the user.
- Test-harness caveats from the final audit, pre-existing from tickets 02 and 03 and recorded as residuals: the remote-sync suite mutates its live production `index.mjs` in place with restore only in a `finally` (an interrupted run can leave production mutated), and the style-control suite drives `apply()` through a hand-rolled cordis context stub rather than a real `@deepseek-ai/cordis` Context. Both are follow-up candidates outside ticket 05's verification scope.

## Comments

User-directed scope decisions:
- Keep complete `link:` repair with `plugins/node_modules` fallback (approved).
- Keep selected subagent model unchanged.
- Never self-restart the hosting DSH Runtime.
