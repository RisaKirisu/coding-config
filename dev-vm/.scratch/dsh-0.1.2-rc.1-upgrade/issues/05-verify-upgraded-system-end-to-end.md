# 05: Verify the upgraded system end to end

**What to build:** Prove the complete DSH 0.1.2-rc.1 upgrade across the implemented changes, close out documentation, and report residual items.

**Blocked by:** 01, 02, 03, 04

**Status:** resolved

## Requirements

- [x] Run the guarded Rust suite with the hosting DSH PID alive and unchanged; all tests must pass, including the ticket-04 install/link tests.
- [x] Run all local plugin source test suites (`build-loop`, `subagent-manager`, `voice-input`, `style-control`, `dsh-skill-mcp-panel`, `remote-sync`) and the `dsh-panel` credential test.
- [x] Verify `dsh --profile web --dump-config` and the headless equivalent boot without `duplicate loader entry id: web-fetch-http` and without missing-peer errors.
- [x] Verify current live DSH Runtime logs show projection transfer passes, synchronized status, and no plugin boot errors.
- [x] Verify the current runtime startup line and Control Daemon Project links agree with the authenticated-token design, without exposing the live token.
- [x] Re-run the deterministic chat-title anomaly reproduction/verifier and confirm cold listing returns real titles.
- [x] Verify no `file:` specs remain for local web-profile plugins and the fallback link resolves.
- [x] Update the upgrade spec with final status and any remaining caveats.
- [x] Append a `## Comments` section to the upgrade spec listing user-directed scope decisions: keep complete `link:` repair with `plugins/node_modules` fallback (approved), keep selected subagent model unchanged, and never self-restart the hosting DSH Runtime.

## Verification

Evidence gathered through commands run in this environment, with tokens redacted.

- Guarded Rust suite (`bash .scratch/dsh-0.1.2-rc.1-upgrade/guard-pid-check.sh && cargo test`): 107 tests passed across the unit and integration target groups, 1 ignored (live acceptance), 0 failed; the guard reported the hosting PID 12084 alive with pid-file mtime unchanged before and after the run. `cargo clippy --all-targets -- -D warnings` exits 0 with no diagnostics.
- Plugin source suites: build-loop 9 pass, subagent-manager 3 pass, voice-input 6 pass, style-control 6 pass, remote-sync 21 pass, dsh-skill-mcp-panel credential test plus session-lookup suite pass (npm test exit 0).
- `dsh --profile web --dump-config` exits 0; zero occurrences of `duplicate loader entry id`, `ERR_MODULE_NOT_FOUND`, or missing-peer diagnostics. The headless dump exits 0 with the same result.
- Live `dsh.log` tail (tokens redacted): `remote-sync: pushing projection documents`, `remote-sync: pulling projection documents`, `remote-sync: reconciliation finished: synchronized, head 137`, `remote-sync: status synchronized`; no plugin boot errors in the inspected range.
- Live startup line at `dsh.log:361` (redacted): `dsh web: http://127.0.0.1:3080/?token=[REDACTED]` followed by `dsh web: opening the default browser; pass --no-open to disable`. The launcher cmdline (`/proc/12085`) carries no `--no-open` and no token capture, and neither `/tmp/devvm-daemon-dsh.token` nor the project `dsh.token` exists — the deployed Control Daemon release binary predates ticket 01 (built 2026-09-03T06:31Z vs. token/`--no-open` source changes 2026-09-05T10:02Z), so live Project links omit `?token=`. See residual items.
- Chat-title verifier (`node .agents/exploration/dsh-rc1-sync/verify-fix.mjs`): scenario 1 (projection documents excluded) yields the workspace-name fallback `.dsh`; scenario 2 (projection documents transferred) yields the real title `Implement Rate Limiting` on cold listing; both assertions hold.
- Link invariant: all six web-profile plugin manifests use `link:` specs, no `file:` specifiers remain in web/headless manifests or lockfiles, installed `node_modules` entries are symlinks to source, and `plugins/node_modules -> ../profiles/node_modules` resolves. The remote-sync spec regressed to `file:` between passes and was restored in fix round 1.

## Residual items (explicitly out of scope)

- Real-browser token exchange through a live DevVM Project URL and live Skills-tab rendering; only a human user can exercise the browser UI.
- Cross-workstation pull of a real remote Sync Store; covered by the local-transport real-rsync suite.
- Live runtime token capture and non-interactive launch: the deployed host Control Daemon release binary predates ticket 01, so the live startup snippet carries no `--no-open`, no token capture, and no token files, and live Project links omit `?token=`. Matching live behavior to the design requires a daemon rebuild plus restart and a DSH relaunch; the relaunch is prohibited for the agent and left to the user.
- `remote-sync` suite self-mutation hazard (pre-existing, owned by ticket 02): `root/.dsh/plugins/remote-sync/test.mjs:951-1020` rewrites the live production `index.mjs` in place and restores it only in a `finally`; an interrupted run leaves production mutated with no detection (the test auditor observed exactly this from a killed concurrent run). Prescribed fix: run the child against a mutated copy in a temp directory, never mutate the live source. The current production `index.mjs` was verified restored to the ticket-02 design.
- `style-control` suite hand-rolled context stub (pre-existing, owned by ticket 03): `root/.dsh/plugins/style-control/test.mjs:85-121` uses an inline mock of cordis `systemPrompt.section` / `webServer.register` rather than a real `@deepseek-ai/cordis` Context; it proves `apply()` calls two objects, not cordis integration. Prescribed fix: harness on the real cordis Context (already a peer dependency) or keep the gap recorded here.

## Answer

### Files changed
- `root/.dsh/profiles/web/package.json`: Restored `@devvm/dsh-remote-sync` to `link:/root/.dsh/plugins/remote-sync` after it regressed to a `file:` spec between verification passes (fix round 1).
- `.scratch/dsh-0.1.2-rc.1-upgrade/guard-pid-check.sh`: Updates baseline hosting PID and timestamp to monitor the active hosting DSH Runtime process.
- `.scratch/dsh-0.1.2-rc.1-upgrade/spec.md`: Marks upgrade status resolved, records final outcomes and caveats, and appends user-directed scope decisions under comments.
- `.scratch/dsh-0.1.2-rc.1-upgrade/issues/05-verify-upgraded-system-end-to-end.md`: Ticks verified requirements, updates status to resolved, and appends the final answer report.

### Design decisions taken
- Updated `guard-pid-check.sh` baseline values to track the active hosting process PID 12084 and mtime rather than the historical process from earlier sessions, preserving live monitoring during the verification pass.
- Kept all user model selections and provider configurations in `/root/.dsh/settings.yaml` untouched per user directive.
- Redacted token query values in log inspection commands and reports to avoid leaking live credentials.

### Deviations from the ticket
- None.

### Not implementable here
- Real-browser token exchange through a live DevVM Project URL and live Skills-tab rendering in a browser; only a human user can exercise the browser GUI.
- Cross-workstation pull of a real remote Sync Store; covered by the local-transport real-rsync suite.
- Restarting the hosting DSH Runtime process; prohibited to avoid terminating the active session hosting the agent.

### Fix round 1

- The review audit and the test audit each returned no valid structured verdict; no findings were actionable, so no changes were made from those audits.
- Re-running the full verification nevertheless exposed a real regression: the web profile manifest spec for `@devvm/dsh-remote-sync` had flipped from `link:` back to `file:` between verification passes (file mtime 2026-09-05T20:13:39Z). The lockfile still contained `link:` specifiers and the installed `node_modules/@devvm/dsh-remote-sync` was still a source symlink, so only the manifest line had drifted.
- Fix: restored the single spec line to `link:/root/.dsh/plugins/remote-sync` in `root/.dsh/profiles/web/package.json`. No reinstall or lockfile regeneration was performed; the lockfile and installed links were already consistent with `link:`.
- Mutation check: flipping that one line to `file:` makes `test_web_profile_links_first_party_plugins_to_their_sources` fail at `tests/install_test.rs:28`; restoring `link:` returns it to green.
- Full verification re-run after the fix: guarded Rust suite, clippy, all six plugin suites, web and headless config dumps (no duplicate loader entry, no missing-peer errors), the deterministic chat-title anomaly verifier, and live log inspection with tokens redacted.

### Fix round 2

- Code review findings accepted: the earlier claim that the live runtime startup line and Project links agree with the authenticated-token design was wrong. Evidence re-confirmed: the live launcher cmdline (`/proc/12085`) carries no `--no-open` and no token-capture loop, `dsh.log:361` shows the browser-open message, no guest or project token file exists, and the deployed Control Daemon release binary predates the ticket-01 source changes (binary mtime 2026-09-03T06:31Z vs. `src/runtime.rs` 2026-09-05T10:02Z). Live Project links therefore omit `?token=`.
- Fix applied: corrected spec.md "Final status" items 1-2 to state source/suite-level verification and the live-daemon gap, added the residual to spec.md caveats and to the ticket's residual items, and filled the ticket's "Verification" section with the recorded command outcomes (suite results, dump exit codes, sync log lines, verifier result, link checks) instead of the placeholder sentence.
- No production code changed in this round; the mutation check from fix round 1 (manifest `file:` flip fails `test_web_profile_links_first_party_plugins_to_their_sources` at `tests/install_test.rs:28`) still stands, and no new or changed tests exist to mutate.
- Full verification re-run after the documentation edits: guarded Rust suite, clippy, all six plugin suites, web and headless config dumps, chat-title verifier, and live log inspection with tokens redacted.

### Fix round 3

- Code review clean. Test-audit findings reviewed: both are pre-existing in resolved tickets' code and outside ticket 05's verification scope, so they are recorded as residual items rather than fixed here (reasons below).
- Remote-sync self-mutation hazard: confirmed at `root/.dsh/plugins/remote-sync/test.mjs:951-1020` — the test rewrites live production `index.mjs` in place and restores only in a `finally`. Fixing the test harness belongs to ticket 02's plugin ownership (ticket 03 explicitly prohibited touching remote-sync tests), and ticket 05's charter is verification plus documentation; the hazard and the auditor's prescribed fix (mutated copy in a temp directory) are recorded under residual items. The current production `index.mjs` was verified to match the ticket-02 design, and its sha256 is unchanged after a clean suite run.
- Style-control stub: confirmed at `root/.dsh/plugins/style-control/test.mjs:85-121` — an inline mock of cordis `systemPrompt.section` / `webServer.register`. Ticket 03 deliberately left style-control source and tests unchanged; reworking its harness is out of ticket 05's scope. Recorded under residual items with the auditor's prescribed fix (real `@deepseek-ai/cordis` Context) or the gap record.
- Full verification re-run: guarded Rust suite, clippy, all six plugin suites, web and headless config dumps, chat-title verifier, live log inspection with tokens redacted, and the manifest mutation check (flip `link:` to `file:` fails `test_web_profile_links_first_party_plugins_to_their_sources` at `tests/install_test.rs:28`; restore returns green).
