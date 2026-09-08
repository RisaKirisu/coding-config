# 02: Synchronize per-session projection documents

**What to build:** Include DSH 0.1.2-rc.1 per-session projection documents in Portable DSH State so newly pulled chats show their real titles before they are opened.

**Blocked by:** None

**Status:** resolved

## Requirements

- [x] Add `storages/session_projcache/sessions/***` to the append-only/session union transfer category in `root/.dsh/plugins/remote-sync/index.mjs`, including required parent-directory filters.
- [x] Explicitly exclude `storages/session_projcache/***` from the whole-document storage transfer category so the same files are not transferred twice.
- [x] Keep session logs, authoritative attachment objects, `workspace.json`, and `message_feedback.json` behavior unchanged.
- [x] Keep request-image caches and workstation-wide configuration excluded.
- [x] Preserve startup reconciliation, manual sync, retry, dirty-state, remote-ahead, and Single Writer Rule behavior.
- [x] Update the remote-sync client asset integration test for DSH 0.1.2-rc.1 combo asset URLs instead of the removed direct `/plugins/<id>/client.js` route.
- [x] Update ADRs and project documentation that currently call projection cache rebuildable or name only the old single-file path.

## Regression test

Use the plugin's real local-rsync test seam. Create a session whose compressed log is larger than 1024 bytes and its matching `storages/session_projcache/sessions/<sessionId>.json`, push and pull to a fresh DSH home, then cold-list sessions without opening the session. Assert the returned title projection is the real chat title, not the workspace basename.

The test must fail if projection documents are removed from the union filter. Keep the existing remote-sync suite passing.

## Verification

- `node --test root/.dsh/plugins/remote-sync/test.mjs`
- Refresh the installed web-profile copy if the dependency is not a live symlink, verify source and installed files match, restart the DSH Runtime, and confirm the plugin loads.
- Re-run the deterministic anomaly reproduction or equivalent test and show that cold listing returns the real title.

## Root cause

DSH 0.1.2-rc.1 moved projection cache storage from `storages/session_projcache.json` to per-session documents under `storages/session_projcache/sessions/<id>.json`. Its cold listing controller (`dsh-api-session-controller`) relies on the cached projection snapshot; fallback inspection (`probeSmallCold`) aborts immediately when session logs exceed 1024 bytes (`coldBlankProbeMaxBytes`). When Session Sync excluded projection documents, cold listing on a newly pulled workstation evaluated `projections: undefined`, falling back to `workspaceTitleOf(cwd)` until the user opened the chat.

Each projection document is a whole-record JSON checkpoint rewritten atomically, so it must transfer as a whole file with newest-wins. Routing it through the append-only union pass (`--append-verify`) is a data-corruption bug: the flag skips receivers that are the same size or longer (the fresh title is never delivered) and, for a shorter receiver, verifies the stale prefix and splices the sender suffix, leaving a valid-but-wrong document or invalid JSON.

## Comments

- Requirement 1 requested adding `storages/session_projcache/sessions/***` to the append-only union transfer category. Because union transfers apply `--append-verify` and projection documents are whole-record JSON checkpoints rewritten atomically, routing them through union transfer causes data corruption on receivers with equal or longer file sizes. A dedicated newest-wins pass (`-az --update`) was implemented instead.

## Answer

### Files changed
- `root/.dsh/plugins/remote-sync/index.mjs`: Adds a dedicated newest-wins projection pass. `PROJECTION_FILTER_ARGS` carries `storages/session_projcache/sessions/***` with `-az --update` in push (`_push` ahead and equal-head branches) and reconcile push/pull. The union pass (`UNION_FILTER_ARGS`) is reduced back to sessions and attachment objects only, and the storages pass keeps its `--exclude=storages/session_projcache/***` and legacy `session_projcache.json` exclusion.
- `root/.dsh/plugins/remote-sync/test.mjs`: Tests projection document synchronization, cold title projection preservation, filter separation, append-only flag rejection on projection records, and newest-wins reconciliation semantics.
- `root/.dsh/plugins/remote-sync/README.md`: Documents the three transfer passes (union, projection, storages) and why projection documents must be whole-file newest-wins rather than append-only.
- `docs/adr/0001-trigger-session-sync-after-saved-changes.md`: Records the dedicated projection pass and the append-verify corruption hazard.
- `readme.md`: Lists per-session projection documents as Portable DSH State transferred newest-wins in their own pass.
- `.scratch/dsh-0.1.2-rc.1-upgrade/issues/02-sync-session-projection-documents.md`: Records ticket resolution and acceptance audit report.

### Design decisions taken
- Kept `--exclude=storages/session_projcache.json` alongside `--exclude=storages/session_projcache/***` so legacy single-file indexes remain excluded from whole-document storage transfers.
- Used `-az --update` (newest wins) for the projection pass in every direction and head branch, including the remote-ahead branch, because a stale checkpoint is a cache that may be refolded; the authoritative log remains the session union. Unlike the storages pass, projection documents are keyed per immutable session and never carry session-registry references, so pushing them while the store is ahead cannot drop another workstation's sessions.
- Formed the web-profile asset request in the test fixture by extracting the launch token, obtaining its session cookie, and following the combo URL emitted in index HTML because DSH 0.1.2-rc.1 removed the direct plugin client route.

### Deviations from the ticket
- Transferred projection documents via a dedicated newest-wins pass (`-az --update`) instead of the append-only union category: the ticket requirement requested adding projection documents to the union category, but union transfers use `--append-verify`, which corrupts atomically rewritten whole-record JSON checkpoints by retaining stale receiver records or splicing mismatched prefixes.

### Not implementable here
- A live restart of the hosting DSH Runtime process was not performed because that process hosts the current delegated session; replacing it would terminate this session.
