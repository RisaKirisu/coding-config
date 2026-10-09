# Project app

## Ownership and navigation

The Control Daemon serves the installable app at `/` and management at `/manage`. Use `http://control.devvm.localhost:8100` locally or `https://devvm.<remote-domain>` over the tailnet, as described in [remote access](remote-access.md) and [ADR 0002](adr/0002-use-a-tailnet-scoped-loopback-facade.md). Control and Project origins must be same-site for DSH's `SameSite=Strict` authentication cookie. A secure browser context is required for installation and service workers.

The shell owns Project navigation, lifecycle submission, Logs, connection status, and browser text recovery. The daemon owns registration, observed VM/DSH/Sync status, launch links, and Project Logs. Native DSH owns Sessions, titles, history, settings, rendering, transport, accepted prompts, and Session Sync. Registration, open-port, and Sync management remain in `/manage`, opened in a shell dialog iframe.

The token-free route is `/#/projects/<Project UUID>[/sessions/<encoded Session ID>]`. Explicit navigation updates browser history; native selection replaces the current route. Back/Forward flushes the outgoing draft before selecting. The last Session visited in another Project is held only in memory. The child verifies an exact requested Session through authenticated native `remote.session.projections({sessionId})`, a read-only lookup that does not activate an Agent. Only a successful null result produces the missing-Session notice; failed reads report that availability could not be verified and remain retryable. A confirmed missing target does not create a replacement or retarget text. Selection replies are scoped to the current request, navigation revision, Project, Session, and child instance. Successful selection/navigation clears only selection-related notices; unrelated storage/lifecycle/status notices remain. Frame reconciliation is single-flight across refresh/navigation callers. Retirement checks the current frame identity and selection after saving, so concurrent cleanup cannot remove a replacement or newly selected frame. A stopped Project offers an explicit Launch action; selecting it does not start its VM or DSH implicitly.

There is at most one frame per Project. The selected frame survives disconnect and stopped/starting status. An inactive frame is flushed and released when its Project disappears, its VM/DSH stops, or no pending text remains. Blocked records count as pending and can retain an inactive running frame. Recovery creates delivery frames only for Running Projects; selecting one promotes its existing frame. There is no timer-based hidden-frame retention limit. Browser suspension pauses execution; continuous Android background delivery is not guaranteed.

Project rows show names and status summaries, and search matches name/path. Detailed path, Sync, and lifecycle information is in Project actions. Control Daemon availability and DSH connection state are separate indicators.

## Controls, drawers, and viewport policy

- Search has a neutral focus border. Manual refresh performs a real Project-status fetch with a tooltip, busy spinner, and completion/error notice. Background polling remains quiet, with one request in flight and a five-second interval; polling pauses while hidden/offline and resumes on foreground/network return.
- Lifecycle choices are separate shaded, bordered rows. Manage Projects uses the DevVM webpage symbol.
- The outer header contains one DevVM menu button with three left-aligned lines of decreasing length. The DSH sidebar button is in `conversation.header.leading`. The header has a 44px desktop minimum and 48px narrow/coarse-pointer minimum, with safe-area padding. Mobile header controls retain 44px targets. The loading indicator is a round 8px dot within a 44px button.
- DevVM navigation is a panel at outer widths of at least 960px and a drawer below that threshold; reported viewport segments can make the outer layout wide. Its open/close transition is 220ms. At child widths below 1024px, native DSH navigation is an overlay with content width `min(280px, viewport width - 48px)`. Its wrapper and content widths match, excluding the native border. The native drawer slides and its backdrop fades over 220ms. Reduced-motion preferences disable these transitions.
- At child widths up to 1023px, a one-finger right swipe anywhere in DSH opens native navigation; a left swipe anywhere closes it. Both require at least 64px horizontal movement and horizontal movement greater than 1.5 times vertical movement. Dominant vertical motion beyond 16px and multi-touch cancel tracking. Capture-phase listeners cover both drawer content and backdrop. Opening Project navigation closes native navigation; opening native navigation closes the outer Project drawer on narrow outer layouts.

The outer document requests `interactive-widget=resizes-content`. At visual viewport scale exactly 1, the shell updates its height and top offset from `visualViewport` resize/scroll events without replacing the frame. Pinch zoom leaves the last layout sizing intact and preserves ordinary browser magnification/panning. This is the implemented keyboard policy; fixture height changes do not establish physical Android IME behavior.

Reported vertical hinges reserve the gap between Project navigation and conversation. A horizontal hinge keeps the app in the lower segment. Browsers without segment reporting use the ordinary responsive layout. Embedded narrow/coarse-pointer conversation/composer text has a 16px minimum, retains larger native preferences, and preserves monospace code. Safe areas, system text scaling, native panels, and overflow remain part of device acceptance.

## Native integration and bridge

The [Project-app package](../root/.dsh/plugins/project-app/README.md) is a Web-profile file dependency. Its host entry validates exact `DEVVM_CONTROL_ORIGINS` and optional `DEVVM_CONTROL_DOMAIN`, then injects them with `DEVVM_EMBED_PROJECT_ID` through `webserver/index-inject`. The parent sets the iframe name to its exact origin and retains `no-referrer`; native login redirects strip referrers but preserve the frame name. The child accepts the claimed origin only when it is exactly listed or is an HTTPS/default-port `devvm` or DNS-valid `devvm-*` Control label under the configured domain. Announcements target that one exact origin, and every incoming message must carry that actual browser-reported origin and come from the parent window. Missing or unauthorized names leave the bridge inactive; a spoofed allowed name cannot receive announcements or attach from another origin. It uses the native module loader, React runtime, slots, and `sessions`, `connection`, `conversation`, `uiWorkspace`, and `layout` services, plus the authenticated `remote.session` namespace.

Messages use this envelope:

```js
{protocol: 'devvm-embed', version: 1, projectId, channelId, instanceId, kind, payload, commandId}
```

`instanceId` identifies the child document; `channelId` identifies the parent attachment. The child first announces `available` with its document identity. The parent sends `attach` with a channel; `ready` establishes that document/channel pair. These handshake messages have distinct validation rules. Later traffic requires both identities, in addition to exact origin, source window, Project UUID, protocol, and version. Reconnect generation is separate from attachment identity. `commandId` correlates `result` replies; requests time out after 15 seconds. Successes carry the command's result body; failures carry `{error}`. Use exact `postMessage` target origins.

| Direction | Message kinds |
| --- | --- |
| Parent → child | `attach`, `select-session`, `prepare-switch`, `deliver`, `open-navigation`, `close-navigation`, `reauthorize`, `foreground`, `result` |
| Child → parent | `available`, `ready`, `connection-changed`, `selection-changed`, `navigation-opened`, `error`, `draft-load`, `draft-save`, `prompt-admit`, `prompt-settled`, `prompt-waiting`, `prompt-blocked`, `result` |

| Command | Payload / result |
| --- | --- |
| `draft-load` | Session identity → `{text, revision}` |
| `draft-save` | `{sessionId, text, revision}` → commit acknowledgement |
| `prompt-admit` | `{sessionId, text, mode, requestId, draftRevision}` → `{requestId, revision}` |
| `deliver` | `{sessionId, text, mode, requestId}` → native dispatch result |
| `select-session` | `{sessionId}` → selection result |
| `reauthorize` | `{url}` → same-origin token exchange and native reconnect |

The parent additionally validates daemon-provided launch URL protocol/host. Reauthorization accepts only the child origin's root URL with one token parameter. Explicit field checks exist for draft/admission/delivery bodies; several notification/result bodies are consumed without comprehensive schema validation. Do not describe the bridge as a generic RPC proxy or universally schema-validated. [The shell](../src/web/app.js) and [client bridge](../root/.dsh/plugins/project-app/client.js) own the exact handlers.

During disconnected/connecting recovery, an authenticated child-origin index probe runs every five seconds to distinguish lost authorization from an unreachable runtime. The parent exchanges a fresh launch token inside the same frame, then reconnects native transport. The owner patch resets authoritative sequence watermarks while retaining displayed Session, history, and title until new baselines arrive. History is never cached in the Control text store. Token URLs and authenticated responses never enter the public shell cache.

## Browser text and delivery

[The text store](../src/web/text-store.js) uses IndexedDB database `devvm-text`, schema version 1. Draft keys are `[Control origin, Project UUID, Session ID]`; pending keys append the request ID. Transactions use strict durability and resolve on commit. Draft writes reject lower revisions. Admission checks the existing draft revision, atomically adds pending text, and clears/increments the canonical draft. A per-Session `nextPromptOrder` orders pending records; there is no global FIFO.

The parent commits admission before acknowledging it. Native input clears only after that acknowledgement, then dispatches through the ordinary native pipeline with the captured Session, text, Queue/Steer mode, and stable request ID. The native embedded persistence provider disables independent child draft persistence/seeding. Outside embedding, native draft behavior is unchanged. Slash commands and attachment-only operations retain their native semantics and are not replayed as durable plain text; image/file bytes are not in this recovery store.

Persisted pending states are `waiting` and `blocked`. In-flight claims, dispatch ownership, and confirmation work are held in browser memory. Eligible records are dispatched without awaiting each prior delivery, so sorted records do not imply serial host acceptance. Native admission coalesces concurrent requests by Session/request ID and recognizes accepted IDs in inbox/history, including after restart; IndexedDB alone does not provide duplicate-acceptance protection.

Native success removes the record after parent commit acknowledgement. Disconnect, `gateway/internal`, and thrown delivery errors retain it for retry. Other connected rejections persist blocked with their error. Retry changes blocked to waiting without changing Session/request identity. Authentication failure behavior follows the native connection/error classification; it is not universally a blocked state. Background delivery never starts a stopped runtime. Changing the current send preference does not rewrite a pending record's captured mode.

Two requested storage behaviors remain unimplemented: the shell does not request persistent browser storage, and cold-start saved-text initialization depends on a successful daemon Project-status fetch. A cached shell can open offline, but saved-text availability is not independently initialized then. Browser eviction/deletion remains possible. These are open requirements in [the spec](../.scratch/devvm-project-app/spec.md).

## Lifecycle and public app updates

`POST /api/projects/{id}/operations` accepts `{action, request_id: UUID}` and returns a no-store `202` `OperationView`, not a Running guarantee. Shell actions are `start_vm`, `stop_vm`, `delete_vm`, `launch_dsh`, `stop_dsh`, and `restart_dsh`. The response contains `id`, `project_id`, `request_id`, `action`, `state`, `accepted_at`, `finished_at`, and `error`; timestamps are Unix milliseconds, and response `action` is a display label.

States are `accepted`, `waiting_for_cleanup`, `running`, `succeeded`, `failed`, and `cancelled`. `GET` on the same route returns `{daemon_instance_id, active, recent}`. Recent terminal records are newest first and capped at 16 per Project. Legacy request-owned records have a null request ID. Retained records can remain readable after unregister; otherwise a missing Project returns 404. Reusing a retained request ID returns `409 Busy`, not the old receipt. Correlation is bounded by the retained records; the shell does not persist/replay lifecycle commands after uncertain submission.

App work is daemon-owned after submission. Legacy blocking endpoints remain request-owned. Both share [Operations](../src/lifecycle.rs), Project coordination, cancellation, status observation, and logging. Stop reserves the successor before cancellation/cleanup; predecessor ownership is held until cleanup completes. Cancellation cleanup has no execution deadline. In-memory receipts disappear on daemon restart, while VM/DSH status is read from the runtime. DSH stays Starting until the current launch token exists.

The manifest uses a standalone root scope/start URL and Control-origin identity. On a configured remote Control host, its `name` and `short_name`, the initial document title, and the iOS home-screen title equal the Control subdomain: `devvm` or the full `devvm-*` label. Local and unrelated hosts retain `DevVM`. Each origin has independent install identity, public cache, drafts, and pending text; aliases do not migrate that state. Existing installations may require browser metadata refresh or reinstallation to adopt a changed name. The service worker caches only explicit public shell assets; Control APIs, authenticated launch data, and sibling DSH responses remain network-only. Its version hashes compiled shell assets. A waiting update asks users to close DevVM windows and reopen; it never forces an active refresh. Schema migrations must preserve saved text.

Shell assets are compiled into [the daemon](../src/ui.rs), so shell edits require rebuilding/redeploying that executable. Native package replacements and installed local-plugin snapshots require a fresh target DSH process. [Runtime maintenance](dsh-runtime-maintenance.md) owns image, existing-VM, and release-upgrade procedures.

## Settings persistence

The Web profile configures `ui-conversation.config.busyEnter: steer` and `locale.config.preference: zh` in [its patch](../root/.dsh/profiles/web/cordis.patch.yml). Saved fields belong to the active Runtime profile's patch, not the Control IndexedDB store.

The send field is the native busy-Enter preference. Plain Enter/Send uses it when the agent is busy and supports steering; idle/non-steering submissions use Queue. Cmd/Ctrl+Enter uses the complementary behavior in the steer-capable busy state. Verify the preference in General settings/profile separately from a current delivery-mode label.

Published DSH `0.2.0-rc.2` selects native form persistence from `remote.$host.isLoopback`. Its served-browser flag comes from the page hostname. Loopback pages use profile-backed forms; non-loopback pages use memory-only forms, skip profile reads, and make queued writes inert. Language/send controls still update local UI immediately. When state is recreated without the profile projection, locale uses the browser-derived language with English fallback and busy-send uses Queue. The English-browser reproduction restores English/Queue despite correct profile fields. This occurs in ordinary remote DSH pages as well as embedding.

No declared native configuration or supported Cordis consumer API changes that policy in this release. Remote native preference persistence remains an unmet requirement. Do not claim that rewriting the profile or restarting processes fixes it. The native Settings API's authentication and loopback facts must remain accurate. [Maintenance](dsh-runtime-maintenance.md#settings-policy-admission) records the relevant public API boundary and future-version admission check.

## Verification

Run only checks affected by a change. Lifecycle/daemon changes use [API](../tests/api_test.rs), [lifecycle](../tests/lifecycle_test.rs), and [observability](../tests/observability_test.rs) tests. Dependency/export changes use [the real frozen installation check](../tests/dsh-plugin-install.test.mjs); installation alone does not validate native patches or CSS.

The browser carrier uses real Chromium, Caddy, the daemon, native DSH, and published native model replay with [the recorded fixture](../tests/fixtures/README.md). Only the VM-command boundary is the owned platform fixture. Homes, registries, ports, PID files, logs, and children belong to `.agents`; never operate or probe the Runtime hosting the agent session.

Prepare a fresh independent stage, then keep its owned carrier running:

```sh
mkdir -p .agents/tmp
DEVVM_DSH_CLI=/absolute/pinned/dsh/lib/bin.js node scripts/prepare-project-app-browser.mjs .agents/project-app-browser/stage
DEVVM_BROWSER_FIXTURE_ROOT="$PWD/.agents/project-app-browser/run" TMPDIR="$PWD/.agents/tmp" timeout --kill-after=5s 19m cargo test --test project_app_fixture -- --ignored --nocapture
```

Preparation accepts a pristine CLI or one already carrying the exact current owner patch; an older-patched source must first be restored on an owned copy. It never patches the installed source CLI. After `ready.json` exists, choose the affected checks:

```sh
export DEVVM_BROWSER_FIXTURE_ROOT="$PWD/.agents/project-app-browser/run"
export PLAYWRIGHT_MODULE=/absolute/node_modules/playwright
export TMPDIR="$PWD/.agents/tmp"
node tests/project_app_controls.cjs
node tests/project_app_mobile.cjs
node tests/project_app_selection.cjs
# Broader delivery/recovery changes:
node tests/project_app_browser.cjs
# Diagnostic/admission probe; remote checks currently fail on stock rc.2:
node tests/project_app_settings.cjs
```

[Controls](../tests/project_app_controls.cjs) covers visual controls, real manual refresh, both drawer transitions, right-swipe opening, left-swipe closing inside the drawer/on the backdrop, and reduced motion. [Mobile](../tests/project_app_mobile.cjs) covers loading geometry, native navigation, vertical intent, viewport sizing, draft/focus/route/document retention, and pinch zoom. [Browser](../tests/project_app_browser.cjs) covers delivery identities, admission concurrency, lost response, browser/process recovery, reauthorization, storage failure, and display retention. These are test scopes; report actual completed runs separately.

[Selection](../tests/project_app_selection.cjs) covers real failed lookup/reconnect recovery, confirmed absence, unchanged frame identity, exact draft retention, startup route restoration, selection-notice clearing, unrelated-notice preservation, and a delayed real missing response superseded by Project navigation. Created Sessions are explicitly attached to a real native Workspace, and readiness is checked through the editable view. Its delay preserves the actual native response rather than fabricating availability.

[Settings](../tests/project_app_settings.cjs) uses local and non-loopback URLs against the same real Runtime, reads actual profile fields, and checks reload, remote edits, fresh contexts, reconnect, and owned Runtime restart. Its remote cases expose the documented rc.2 limitation; do not count it as a passing regression suite. The fixture adds a test-only non-loopback hostname resolved to its owned Caddy by Chrome.

Evidence is written beside `ready.json`. Create the fixture's `stop` file to stop only its children; it automatically stops after 18 minutes. Prerequisites are Linux, Node 24, the exact pinned CLI/replay, `patch`, `tar`, npm, Chrome, Playwright, and Caddy at the declared executable path. Replay supports one recorded Session per process; prompt scenarios reset only the owned Runtime through real lifecycle controls. Chromium's quota probe requires 31 seconds for cached write allowance to expire.

[Control-host checks](../tests/project_app_control_hosts.cjs) use the same carrier with `DEVVM_BROWSER_REMOTE=1`. It adds the shipped host Caddy routes on the owned address `127.0.0.2:443`, an internal test certificate authority, and the remote domain `devvm.test`. The browser maps that domain to the owned address, accepts the test CA, and verifies base/named/numeric Control aliases, native token redirects and Strict cookies, exact-origin embedding, browser and cached manifests, and rejected labels/untrusted parent origins. Run with the same fixture root and `PLAYWRIGHT_MODULE` as the other checks:

```sh
DEVVM_BROWSER_FIXTURE_ROOT="$PWD/.agents/project-app-browser/run" \
PLAYWRIGHT_MODULE=/path/to/playwright node tests/project_app_control_hosts.cjs
```

Physical Android/Samsung installation, IME and real hinge behavior, suspension/network handoff, microphone/clipboard/download permissions, external-link flows, and the actual remote FRP/Tailscale path remain separate acceptance. Emulated viewport/hinge/font checks do not establish those behaviors.

## Deployment

Use [runtime maintenance](dsh-runtime-maintenance.md#deployment) for the complete commands. Host setup conditionally rebuilds the architecture-specific image when image inputs are newer, rebuilds/installs the daemon, and handles authorized service restart. Existing VMs keep their installed global CLI; new images affect newly created VMs. A same-version existing-VM patch update needs the exact deployed patch receipt, a zero-fuzz old-to-new transition, forced local-plugin snapshot installation with the guest store, and a fresh target DSH process. A DSH release change also requires replacing the existing VM's CLI; restarting alone does not upgrade it.
