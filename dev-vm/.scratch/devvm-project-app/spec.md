# DevVM Project app specification

Status: Current implemented contracts are described below and in [the app documentation](../../docs/project-app.md). Isolated browser checks establish integration and viewport behavior. Deployment, physical-device/remote-path acceptance, and the open requirements below are not complete. Do not describe this status as full product acceptance.

## Outcome and authority

Provide one installable Control-origin app for Android Chrome, Samsung Internet, and desktop Chromium, with Project management and the selected Project's native DSH conversation in the same app. Switching Projects must preserve exact text identity; reconnect must retain the displayed conversation until authoritative server state returns.

The Control Daemon owns registration, observed VM/DSH/Sync status, lifecycle work, launch links, and Project Logs. Each DSH Runtime/profile owns Sessions, titles, accepted prompts, history, saved settings, rendering, and native transport. The Control browser store contains only unsent drafts, explicitly submitted text awaiting acceptance, and necessary delivery identity/revision metadata. It never stores transcripts, launch tokens, settings changes, or lifecycle commands.

Project UUID is the text identity and is distinct from `project_host`. Navigation uses validated daemon-provided links. Running requires the current launch's ready token; a live guest process without that token is Starting. Operation state and observed runtime status remain distinct.

## Application and controls

The app is served at `/`; registration, open-port, and Sync management remain in `/manage`, shown inside a management dialog. The outer route is `/#/projects/<UUID>[/sessions/<encoded Session ID>]`, contains no token, and preserves Back/Forward. Native Session selection replaces the current route. Last Session per Project is held in memory. Requested Session availability is checked through the native read-only projection endpoint. Failed reads remain retryable; only a successful null result produces a missing notice without replacement or saved-text retargeting. Selection errors belong to the current Project/Session request, document instance, and navigation revision. Successful selection/navigation invalidates only selection notices, preserving unrelated errors. Frame reconciliation has one in-flight owner and rechecks selection before retirement.

Project rows expose name and status summary, and search matches name/path. Project actions expose detailed path/Sync/lifecycle information and Logs. Opening a stopped Project offers explicit Launch; selection never implicitly starts its VM/DSH. Control availability and the selected DSH connection are displayed separately.

Required visual controls, implemented in the shell:

- Project search uses a neutral focus border without a green outline.
- Manual refresh fetches Project status, shows its tooltip and busy spinner, disables repeated submission, and reports success or daemon failure. Background polling does not show those notices.
- Project action choices are separate shaded, bordered rounded rows with hover/active feedback. Manage Projects uses the DevVM webpage symbol.
- The outer header has one DevVM menu button with three left-aligned horizontal lines of decreasing length. DSH's own sidebar button lives in its native conversation header.
- Header minimum height is 44px on desktop and 48px on narrow/coarse-pointer layouts, with safe-area padding and 44px mobile header controls. The loading status is an 8px round dot inside a 44px button.
- DevVM panel/drawer transitions last 220ms. Embedded narrow DSH content is capped at `min(280px, viewport width - 48px)` with a matching wrapper content width; its native border is separate. DSH slides and its backdrop fades over 220ms. Reduced motion disables transitions.

The outer wide threshold is 960px, or a wide layout formed by reported viewport segments. The child native narrow threshold is below 1024px. A one-finger right swipe anywhere inside a narrow DSH page opens its sidebar; a left swipe anywhere closes it. Each requires at least 64px horizontal movement, with horizontal movement greater than 1.5 times vertical movement. Dominant vertical movement beyond 16px and multi-touch cancel tracking. Native opening closes the narrow Project drawer, and Project opening closes native navigation.

## Typography, viewport, and device behavior

Use the native DSH visual language, font family, rendering, and monospace code. Embedded narrow/coarse-pointer conversation/composer text has a 16px minimum and preserves larger native preferences. Code/tables scroll inside their content. Safe areas and comfortable touch targets remain required.

The outer document requests `interactive-widget=resizes-content`. At `visualViewport.scale === 1`, resize/scroll update shell height/top without remounting the frame. Pinch zoom retains browser magnification/panning. Reported vertical hinges reserve a gap between Project navigation and conversation; a horizontal hinge places the app in the lower segment. Without segment reporting, the ordinary responsive layout applies.

Fold/unfold, rotation, window resize, split screen, and constrained height must preserve Project/Session, committed text, pending ownership, and frame/document identity. Reading position/focus must remain where the browser permits. Real IME, installation, hinge, text scaling, background suspension, and permission flows require separate physical-device acceptance; desktop emulation establishes only its tested geometry/policy.

## Native bridge and frame lifetime

Use native authentication, RPC, stream multiplexer, recovery, Session services, and prompt rendering. The parent has no cross-origin DSH transport or generic RPC bridge. [The app contract](../../docs/project-app.md#native-integration-and-bridge) is the exact envelope/message/payload reference, including the `available` → `attach` → `ready` handshake and 15-second request timeout.

Trusted Project identity and allowed exact Control origins come from host-injected launch metadata. Parent and child validate origin/window, protocol/version, Project UUID, attachment channel, and child-document UUID according to handshake state. A reconnect generation is not a new frame identity. Draft/admission/delivery payloads receive explicit field checks; validation of every notification/result body is not implemented. Tests must state the malformed-field cases actually exercised.

Keep one frame per Project. The selected frame survives stopped/starting/disconnected status. An inactive frame is flushed/released when unregistered, stopped, or no pending records remain. Blocked records can retain an inactive running frame; there is no timer bound. Recovery creates missing delivery frames only for Running Projects and promotes an existing frame on selection. Browser suspension pauses background execution; delivery never starts a stopped Runtime.

Owner-level changes retain displayed Session/history/title through generation reset while authoritative baselines resubscribe. Reauthorization exchanges a fresh token inside the same frame and reconnects native transport. An authenticated index probe repeats every five seconds while disconnected/connecting. Neither token URLs nor authenticated responses enter public caching.

## Lifecycle operations

The app submits daemon-owned work through `POST /api/projects/<id>/operations`; legacy blocking lifecycle calls retain request-drop cancellation. Both use the same Operations engine, Project coordination, status observation, runtime actions, logging, and process-group cleanup.

[The app contract](../../docs/project-app.md#lifecycle-and-public-app-updates) records the exact request/receipt/state fields. A `202` receipt does not assert Running. UUID correlation is bounded by active/16 recent terminal records; a retained duplicate request UUID returns 409. No browser lifecycle-command outbox exists. Stop reserves its successor before predecessor cancellation cleanup, and coordination remains held until cleanup completes. Cancellation cleanup has no execution deadline. A daemon restart loses in-memory receipts and independently re-observes VM/DSH status.

## Draft and pending-text contract

IndexedDB `devvm-text` version 1 uses Control origin + Project UUID + Session ID for draft identity, adding request ID for pending identity. Strict transactions resolve only on commit. Draft revisions reject stale writes. Admission atomically adds pending text and clears/increments the canonical draft after checking its revision. Order is per Session.

Embedded native draft persistence/seeding is disabled in favor of the parent provider. A canonical empty parent draft must not resurrect stale child text. Switch flushes before release. Native admission awaits the parent commit before composer clear/dispatch and preserves the native optimistic bubble and exact target, mode, and request ID. Storage failure retains text and prevents dispatch.

Persisted pending states are waiting/blocked; in-flight dispatch/confirmation claims are in memory. Eligible dispatches do not wait for prior delivery, so browser order does not guarantee serial host acceptance. Native admission coalesces overlapping identical IDs and recognizes accepted IDs in recorded inbox/history after restart. Settlement removes the record only after the parent commit acknowledgement. Retry preserves Project/Session/request identity and captured Queue/Steer mode.

Disconnection, `gateway/internal`, and thrown delivery errors preserve retryable text; other connected rejections persist blocked with the actual error. Authentication classification follows native connection/error state. Recovered text never creates a replacement Session or launches a stopped Runtime. Slash commands, attachment-only operations, and file/image bytes are outside durable plain-text replay.

## Installed-app and settings requirements

The manifest has root start/scope, standalone display, icons, and a stable Control-origin identity. Cache only explicit public shell assets. APIs, operation results, launch tokens, and sibling DSH responses remain network-only. A waiting update asks the user to close/reopen DevVM windows; never force reload of an active conversation. Future text-store schema changes must preserve saved text.

Web defaults are Steer (`ui-conversation.config.busyEnter`) and Simplified Chinese (`locale.config.preference: zh`). Saved settings belong to the active Runtime profile. Steer is the busy-Enter preference: native idle/non-steering submissions use Queue; the accelerated chord selects the complementary behavior while steering is supported and busy. Remote preference edits must be durable and survive reload, new browser contexts, reconnect, and Runtime restart without unsolicited default restoration. **That remote requirement is unmet in published rc.2:** its non-loopback form provider skips profile reads/writes, and current supported configuration/Cordis APIs have no persistence override. [Runtime maintenance](../../docs/dsh-runtime-maintenance.md#settings-policy-admission) records the boundary and admission test. A pending message retains its captured mode regardless of later preferences.

## Open requirements and acceptance limits

| Requirement | Current implementation / remaining work |
| --- | --- |
| Durable remote native preferences | Native rc.2 memory-only policy; no supported configuration override. Keep this requirement unresolved until a supported policy passes the real settings probe. |
| Request persistent browser text storage | IndexedDB commits are implemented; no `navigator.storage.persist()` request exists. Browser eviction/deletion remains possible. |
| Saved-text access on offline cold start | Cached shell can open; pending recovery initialization currently depends on successful daemon Project-status retrieval. Independent offline initialization is not implemented. |
| Dedicated unavailable/deleted Session view | Current behavior is an error notice without replacement/retargeting. A dedicated view is not implemented. |
| Host deployment | Repo changes and owned-fixture proof do not establish deployment of daemon, image, existing-VM CLI patches, or plugin snapshots. |
| Physical/remote acceptance | Android/Samsung install/IME/hinge, permissions, suspension/network handoff, external links, downloads, and actual FRP/Tailscale traces remain device/path checks. |

## Acceptance and maintenance checks

- [Controls](../../tests/project_app_controls.cjs): neutral focus, real refresh feedback, selectable action rows, matching management/menu icons, header minima, native button placement, Project/DSH animation, matching drawer width, right-swipe opening, left-swipe closing inside/on backdrop, and reduced motion.
- [Mobile](../../tests/project_app_mobile.cjs): round connection indicator, native navigation/coordination, vertical intent, keyboard viewport policy, draft/focus/route/document retention, and pinch zoom. Run physical Samsung keyboard acceptance separately.
- [Browser carrier](../../tests/project_app_browser.cjs): A→B→A drafts, exact-target pending delivery, canonical empty drafts, quota denial, browser-process recovery, lost receipts, concurrent same-ID acceptance, accepted-ID Runtime recovery, reauthorization, retained history/title/DOM, Retry, and tested bridge failures. Do not use mocked success as integration proof.
- [Selection](../../tests/project_app_selection.cjs): real offline lookup/recovery, confirmed missing target, frame/draft retention, startup restoration, selection-notice clearing, unrelated notices, and late real responses after Project navigation.
- [Settings probe](../../tests/project_app_settings.cjs): one real Runtime with loopback/non-loopback URLs, actual profile reads, remote edits, reload, new contexts, reconnect, and Runtime restart. It currently exposes rc.2 failures and is not counted among passing regressions.
- [API](../../tests/api_test.rs), [lifecycle](../../tests/lifecycle_test.rs), and [observability](../../tests/observability_test.rs): shared detached/blocking semantics, HTTP disconnect, preemption/cleanup, two clients, daemon restart/status readback, token readiness, and Project Logs. Run when those owners change.
- [Frozen native install](../../tests/dsh-plugin-install.test.mjs): package snapshots/ordinary dependencies/runtime peers and imports without source/fallback links. It does not establish owner-patch or CSS compatibility.
- Physical acceptance: Android Chrome/Samsung Internet standalone install/reopen/update; keyboard, text scaling, safe areas, reported hinges, fold/rotation/split screen during drafts/pending/streaming, background/lock/handoff, clipboard/microphone, files/downloads/external links; actual remote ingress/runtime/Tailscale traces. Preserve identities/text and report any device limit directly.

Run affected checks in owned `.agents` fixtures, never against the hosting Runtime. [The app documentation](../../docs/project-app.md#verification) records preparation and proof boundaries. [Runtime maintenance](../../docs/dsh-runtime-maintenance.md) records native owners/generated selectors, release pins, complete old-to-new patch validation, new/existing-VM deployment, snapshot refresh, and future DSH-version admission. Rebase native behavior at its owners and verify it with real published candidate artifacts; patch applicability alone is insufficient.

## References

- [Domain model](../../CONTEXT.md)
- [Control routes](../../src/api.rs), [view models](../../src/models.rs), [lifecycle](../../src/lifecycle.rs), [Runtime management](../../src/runtime.rs)
- [Shell](../../src/web/app.js), [text store](../../src/web/text-store.js), [native bridge](../../root/.dsh/plugins/project-app/README.md)
- [Remote access](../../docs/remote-access.md), [same-site/Tailnet ADR](../../docs/adr/0002-use-a-tailnet-scoped-loopback-facade.md)
