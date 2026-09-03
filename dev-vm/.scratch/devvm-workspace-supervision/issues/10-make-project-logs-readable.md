# 10: Make Project Logs readable

**What to build:** The daemon merges a Project's `daemon.log`, `dsh.log`, and `ingress.log` into one time-ordered list of structured entries, and the Web UI renders them as distinguishable rows with local timestamps, source badges, filtering, and auto-scroll that only follows when the reader is already at the bottom.

**Blocked by:** 09

**Status:** resolved

**Why:** The viewer concatenates the daemon log and a 64 KB ingress tail as raw text, so DSH and daemon lines are buried above a wall of Caddy JSON, timestamps are epoch seconds or embedded JSON floats, and a 2-second refresh forces the scroll position to the bottom, making older lines unreadable.

## Server (`src/logs.rs`, `src/api.rs`)

- `struct LogEntry { ts: String /* ISO-8601 UTC */, source: String /* daemon | dsh | ingress */, level: String /* info | warn | error */, message: String }`.
- `read_recent_logs(log_dir, project_id, max_bytes)` reads the tail of each of the three files, parses, merges by `ts` (stable on ties, file order daemon → dsh → ingress), returns `Vec<LogEntry>`.
- Parsing:
  - `[ISO] [source] text` (daemon.log): `source` from the bracket (`daemon:error` → source `daemon`, level `error`; `sync:warn` → level `warn`, message prefixed `sync:`).
  - `[ISO] text` (dsh.log, frpc lines in ingress.log): level `error` when the text starts with `Error`, `error:`, or contains ` ERROR `; otherwise `info`.
  - Caddy JSON (ingress.log): `ts` seconds float → ISO; `level` from JSON; message compacted to `<method> <uri> → <status> (<duration> ms)` for `http.log.access`/`http.log.error` entries with a `request` object, plus `msg` when it is not the generic `handled request`; other JSON keeps `msg`. Unparseable lines keep the raw text with the previous entry's timestamp.
- `GET /api/projects/{id}/logs` returns `{ project_id, entries: [LogEntry] }`. The `logs` string field is removed; tests that assert on it move to `entries`.

## Client (`src/ui.rs`)

- Modal widens to 1100 px, body 70 vh.
- Toolbar: source filter chips `daemon`, `dsh`, `ingress` (all on by default; state kept while the modal is open), an `errors only` toggle, and a `Follow` toggle.
- Row: `[HH:MM:SS.mmm]` local time in muted monospace, source badge with a fixed colour per source, message in monospace with `white-space: pre-wrap; word-break: break-all`, alternating row background, red-tinted row for `error`, amber for `warn`.
- Refresh every 2 s. After rendering, scroll to bottom only if `Follow` is on, and `Follow` turns itself off when the user scrolls up more than 24 px from the bottom and back on when they scroll to the bottom. Rendering keeps the current scroll offset otherwise.
- Render diff-free is fine (rebuild the list) but preserve `scrollTop` across rebuilds.

## Acceptance criteria

- [x] Unit tests: each of the three line formats parses to the expected `LogEntry`; merged output is time-ordered across files; Caddy request compaction produces `GET /api/events.mux → 502 (0.2 ms)`; unparseable lines are kept.
- [x] API test: `/logs` returns `entries` with entries from all three files.
- [x] Served HTML contains the filter chips, `Follow` control, and the scroll-position logic (`scrollTop`, threshold) — asserted by string presence in `api_test` as the existing UI tests do.
- [x] `readme.md` describes the log viewer briefly (sources, filtering, follow).

## Answer

### Changes

- `src/logs.rs`: added `LogEntry { ts, source, level, message }` (`Debug, Clone, PartialEq, Serialize, Deserialize`) and the plain parsers `parse_daemon_line`, `parse_prefixed_line`, `parse_caddy_line`, `parse_file(source, text, &mut Vec<LogEntry>)`. `read_recent_logs` now returns a `Vec<LogEntry>`: it reads the same per-file `max_bytes` tail (partial first line dropped) for `daemon.log` → `dsh.log` → `ingress.log`, parses each, and merges with a stable `sort_by` on `ts`. `format_iso8601_millis` now takes epoch milliseconds so Caddy's float `ts` and `SystemTime::now()` share one formatter; the old `append_clean_lines` concatenation helper is gone.
  - Daemon tag handling: `daemon:error` → source `daemon`, level `error`; `sync:warn` → level `warn`, message `sync: …`; unknown tags (`devvm`, `devvm:err`) keep their name as a message prefix and report as `daemon`, with `err`/`error` and `warn`/`warning` suffixes mapped to levels.
  - Caddy compaction: `"{method} {uri} → {status} ({duration_ms:.1} ms)"`, arrow part dropped when `status` is absent (`http.log.error`), ` — {msg}` appended when `msg` is present and not `handled request`; `level` mapped `warn`/`error`/else `info`.
  - Unparseable lines keep the raw text with the previous entry's `ts` within the same file, so head-of-file lines get an empty `ts` and sort first (commented at `read_recent_logs`).
- `src/models.rs`: `LogsResponse.logs: String` → `entries: Vec<LogEntry>`; `src/api.rs`: `get_logs_handler` returns `{ project_id, entries }`; `src/lib.rs` re-exports `LogEntry`.
- `src/ui.rs`: logs modal widened to 1100 px with a 70 vh scrolling list; toolbar of `chip` toggles (`data-source` chips for daemon/dsh/ingress, `errors only`, `Follow`) whose state lives in module-level `logSources`/`logErrorsOnly`/`logFollow` and is reset when the modal opens; rows render local `HH:MM:SS.mmm` (via `new Date(ts)`), a per-source colour badge (daemon blue, dsh green, ingress grey) and the escaped message with `pre-wrap`/`break-all`, alternating background, red tint for `error`, amber for `warn`. The list is rebuilt each 2 s refresh, restoring `scrollTop` unless Follow is on; a `scroll` listener sets `logFollow` from `scrollHeight - scrollTop - clientHeight <= 24` and syncs the Follow chip.
- Tests: new `src/logs.rs` unit tests for the three formats, the compaction string, unparseable-line retention and cross-file time ordering; existing log tests moved to `LogEntry` assertions. New `tests/common/mod.rs` helper `log_entries_text` flattens `entries` to `[source] message` lines for the existing substring assertions in `api_test`, `sync_test`, `acceptance_workflow_test`. New `tests/api_test.rs::test_project_logs_merge_three_sources_in_time_order` writes one daemon, one dsh and one Caddy JSON line with interleaved timestamps and asserts three entries ordered `ingress, dsh, daemon` with the compacted message; `test_embedded_ui_served` now asserts `data-source="…"`, `errors only`, `Follow`, `scrollTop`, `<= 24`.
- `readme.md`: paragraph describing the merged viewer, source badges, chips, `errors only` and `Follow`.

### Verification

- `cargo build` clean; `cargo clippy --all-targets -- -D warnings` zero warnings; `cargo test` 89 passed, 0 failed, 1 ignored (`live_acceptance_test` compiles).
- `grep -rn '"logs"' src tests` shows only temp-dir names (`join("logs")`), no API field usage.
- Non-vacuity: removing the merge `sort_by`, replacing `→` with `->`, dropping the previous-`ts` carry-over, ignoring the tag level suffix, renaming `data-source` and changing the threshold literal each failed the corresponding new assertion; all restored afterwards.
- Hosting `dsh web` PID stayed 156 and alive before and after the runs.

### Deviations

- `LogEntry` lives in `src/logs.rs` (re-exported from `src/lib.rs`) while `LogsResponse` stays in `src/models.rs`, matching the ticket's placement of the parser next to the reader.
- Tag names that are not one of the three sources (`devvm`, `devvm:err`) are reported as source `daemon` with a `devvm: ` message prefix; the ticket only specified `sync:`, and the daemon writes those tags too.
- Existing string-based log assertions were kept as substring checks over a flattened `[source] message` rendering (`log_entries_text`) rather than rewritten entry by entry, to keep the migration mechanical.
- `cargo fmt` was run to keep the new code canonical; it also reformatted pre-existing deviations in `src/sync.rs` and `tests/observability_test.rs` (formatting only, tests unaffected).
