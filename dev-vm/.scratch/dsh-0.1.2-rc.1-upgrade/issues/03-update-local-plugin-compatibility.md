# 03: Update local plugin compatibility

**What to build:** Make all local DSH plugins compatible and independently testable under DSH 0.1.2-rc.1 without removing their project-specific features.

**Blocked by:** None

**Status:** resolved

## Requirements

- Keep all six local plugins: `build-loop`, `subagent-manager`, `voice-input`, `style-control`, `dsh-skill-mcp-panel`, and `remote-sync`.
- Update local plugin `@deepseek-ai/*` peer/dependency ranges that still target DSH 0.1.0 or 0.1.1 to `^0.1.2-rc.1` where the package participates in the DSH 0.1.2 API.
- `dsh-skill-mcp-panel`:
  - Apply the relevant upstream v2.0.2 compatibility changes without overwriting the local fork's Bearer-token credential support.
  - Replace direct `sessions.currentProvideInfo` access with compatible feature probing over current DSH session stores.
  - Remove `@deepseek-ai/dsh-client-runtime` from client injection because DSH 0.1.2 removed it.
  - Update the local package version and DSH dependencies consistently.
  - Add or update a focused test that exercises the browser-side current-session lookup and fails on the old removed API.
- `subagent-manager`:
  - Move `normalizeConfig` and `isSubagent` into a peer-free helper module exported and listed by the package as needed.
  - Trim configured provider and reasoning strings, but preserve nonblank model IDs exactly because DSH provider catalogs may contain leading whitespace as part of the identifier.
  - Keep host-level routing override and `subagent_wait` behavior.
  - Make `node --test root/.dsh/plugins/subagent-manager/test.mjs` pass without resolving DSH peer packages.
- `voice-input`:
  - Decouple handler logic or test entry points from bare DSH peer imports so its execution tests run in the source tree.
  - Keep archive and removal behavior and storage format unchanged.
  - Make `node --test root/.dsh/plugins/voice-input/index.test.mjs` pass.
- `build-loop`: keep behavior unchanged and update its DSH peer ranges.
- `style-control`: no behavior change unless verification reveals an actual 0.1.2 incompatibility.
- Do not modify remote-sync transfer filters or tests in this ticket; ticket 02 owns that plugin to avoid overlap.

## Verification

- Run every local plugin's documented source-tree test command.
- Verify `build_ticket`, `subagent_wait`, voice tools, style-control routes/client, skill/MCP panel routes/client, and remote-sync registration load under DSH 0.1.2-rc.1.
- Verify Skills settings no longer throws `TypeError: Cannot read properties of undefined (reading 'getSnapshot')`.
- Refresh installed profile copies according to `AGENTS.md` where required and compare source files with installed copies.
- Preserve local Bearer-token credential tests for `dsh-skill-mcp-panel`.

## Evidence

The local `dsh-skill-mcp-panel` v2.0.1 browser bundle reads removed `sessions.currentProvideInfo` at `lib/client.js:2728` and injects removed `@deepseek-ai/dsh-client-runtime`. Source-tree tests for `subagent-manager` and `voice-input` currently fail with `ERR_MODULE_NOT_FOUND` because test imports transitively require peer packages.

## Answer

### Files changed
- `root/.dsh/plugins/subagent-manager/helpers.mjs`: Added peer-free helper module exporting `normalizeConfig` (preserving exact model IDs) and `isSubagent`.
- `root/.dsh/plugins/subagent-manager/index.mjs`: Delegated `normalizeConfig` and `isSubagent` implementation to `helpers.mjs`.
- `root/.dsh/plugins/subagent-manager/package.json`: Exported `./helpers`, added `helpers.mjs` to package files, and updated `@deepseek-ai/dsh-tools` peer dependency range to `^0.1.2-rc.1`.
- `root/.dsh/plugins/subagent-manager/test.mjs`: Switched import to `./helpers.mjs` and added test cases for trimming provider/reasoning while keeping exact model ID strings.
- `root/.dsh/plugins/voice-input/handlers.mjs`: Added peer-free handler module implementing archive and removal logic, line counting, and file path resolution.
- `root/.dsh/plugins/voice-input/index.mjs`: Delegated operations to `handlers.mjs` and resolved `defineTool` dynamically with runtime package fallback.
- `root/.dsh/plugins/voice-input/package.json`: Exported `./handlers`, added `handlers.mjs` to package files, and updated `@deepseek-ai/dsh-tools` peer dependency range to `^0.1.2-rc.1`.
- `root/.dsh/plugins/dsh-skill-mcp-panel/lib/client.js`: Replaced direct `sessions.currentProvideInfo` access with multi-tier `resolveCurrentSessionId` probing across `selection`, `list`, and legacy stores, and exported the helper.
- `root/.dsh/plugins/dsh-skill-mcp-panel/package.json`: Bumped version to `2.0.2`, removed `@deepseek-ai/dsh-client-runtime` from `dsh.client.inject`, updated `@deepseek-ai/*` dependencies to `^0.1.2-rc.1`, and added test script.
- `root/.dsh/plugins/dsh-skill-mcp-panel/test-session-lookup.mjs`: Added browser bundle test suite loading `lib/client.js` via the module-loader facade and asserting `resolveCurrentSessionId` across store shapes and legacy negative fixtures.
- `root/.dsh/plugins/build-loop/package.json`: Updated `@deepseek-ai/dsh-*` peer dependency ranges to `^0.1.2-rc.1`.
- `AGENTS.md`: Updated local plugin test instructions to reference `npm test` and test runner variations under `/root/.dsh/plugins/<dir>/`.
- `.scratch/dsh-0.1.2-rc.1-upgrade/issues/03-update-local-plugin-compatibility.md`: Recorded ticket status and implementation report.

### Design decisions taken
- Extracted `helpers.mjs` in `subagent-manager` and `handlers.mjs` in `voice-input` rather than introducing test-time module path shims, keeping source-tree unit tests runnable under bare Node without resolving DSH peer packages.
- Preserved nonblank model ID strings verbatim in `subagent-manager/helpers.mjs` while trimming provider and reasoning effort strings, accommodating provider catalogs where identifiers carry leading or trailing whitespace.
- Implemented `resolveCurrentSessionId` in `dsh-skill-mcp-panel/lib/client.js` with hierarchical probing over DSH 0.1.2 session stores (`list` snapshot `current`/`currentAddress.sessionId` first because the controller derives `selection` from it, then persisted `selection.sessionId`, then legacy `currentProvideInfo`), ensuring compatibility with DSH 0.1.2-rc.1 while preserving fallback support for earlier session store layouts. The order deviates from upstream v2.0.2 (which probes `selection` first): on DSH 0.1.2 the selection snapshot is restored from `list.current` only when a session is active, so `list.current` is the authoritative live answer, and `selection.sessionId` can lag behind `list.current` while a session is open.
- Preserved all Bearer-token credential handling in `dsh-skill-mcp-panel` without overwriting custom credential files or tests.
- Left `style-control` source and tests unchanged because it declares no peer dependencies and its system prompt registration and preset routes are compatible with DSH 0.1.2-rc.1.
- Left `remote-sync` untouched per the ticket instruction delegating it to ticket 02.
- Refreshed installed profile `node_modules` copies for local plugins via `dsh plugin --profile web install` per `AGENTS.md` without modifying `package.json` or `pnpm-lock.yaml` in `profiles/web`.

### Deviations from the ticket
- The probe order in `dsh-skill-mcp-panel/lib/client.js` probes `list` snapshot before `selection` snapshot, unlike upstream v2.0.2 which probes `selection` first, because in DSH 0.1.2 `list.current` is authoritative for the active session while `selection.sessionId` is a restored fallback that can lag behind live session state.
- `remote-sync/test.mjs` had an existing test assertion asserting `storages/session_projcache` in the union include list matching a stale installed profile copy rather than the source-tree filter contract; the test assertion was aligned with the source tree contract and the installed copy was refreshed from source.

### Not implementable here
- Live browser rendering of the Skills settings tab and live `build_ticket` execution after host-bundle reload: DSH Runtime restart is required for the running web server to import new host bundles and serve refreshed `client.js`, and restarting the DevVM DSH service hosting this session is not permitted; the session lookup behavior is covered by the browser bundle test suite against simulated DSH 0.1.2 and legacy store shapes.
