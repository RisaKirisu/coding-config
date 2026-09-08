# 06: Simplify Build Loop audit outcomes and retries

Status: resolved
Blocked by: None

## Approved scope

Modify only the local Build Loop plugin, its focused tests/documentation, and the relevant AGENTS.md invariant. Keep implementation small. Provider retry behavior is completely outside this change; do not inspect, configure, or modify it.

## Required behavior

- runAudit owns up to three audit-child launch attempts total, using a fresh child for each attempt.
- Accept a valid structured verdict. Otherwise accept a normally finished non-empty plain output as clean:false, with the output preserved as report and finding for the builder. A partial message from a failed execution must not count as a completed plain-output success. Inspect actual DSH semantics: requesting outputSchema can map normal completion without structured output to error; ensure the intended plain-output success works without accepting actual failed execution. Use minimal existing local-agent turn evidence if needed, not provider-specific logic.
- All other audit outcomes, including start/result/disposal exceptions and empty/failed execution, retry within three total attempts. Dispose each published child. Caller cancellation exits immediately and is not retried.
- Remove the synthetic invalid-verdict finding that currently consumes a build fix round. Audit launch retries never consume build fix rounds.
- Keep review and test parallel and wait for both so successful sibling results are retained. No new configuration, dependency, or speculative abstraction.
- After any auditor exhausts attempts, return semantic status failed, preserving latest valid build, review, and test reports. Include every failed phase, last cause, and attempt count. Include exactly: Notify the user immediately. Do not retry build_ticket.
- Preserve other existing build/fix behavior; use existing failure rendering where possible. Do not discard prior reports when an audit fails.

## Verification

Add focused meaningful tests for verdict acceptance, actual finished-text fallback, rejection of partial failed output, three-attempt limit, success after retry, cancellation, exhausted failures retaining reports and exact instruction, and cleanup/retry behavior as appropriate. Follow repository pure-test/no-hand-rolled-mocks constraints; factor minimal pure decision logic if needed. Run node --test root/.dsh/plugins/build-loop/test.mjs and relevant lightweight checks. Do not claim live host-plugin verification: it requires user restart.

Update Build Loop README and concise AGENTS.md invariant; load writing-for-agents for AGENTS.md edits. Record files, checks, and any limitations here.

## Constraints

Never restart/stop the hosting DSH Runtime. Never change selected provider, model, reasoning, or retry configuration. Do not modify installed DSH code. Preserve unrelated existing working-tree changes. No live failure injection against production services or source-file mutation in tests.

## Answer

### Files changed
- `root/.dsh/plugins/build-loop/loop.mjs`: Added `textOf`, updated `toVerdict` to remove synthetic findings, added `evaluateAuditOutcome`, `formatAuditFailure`, `runWithRetries`, and updated `renderOutcome` failure text formatting.
- `root/.dsh/plugins/build-loop/index.mjs`: Wired `runAudit` to retry up to three launch attempts using `runWithRetries` and `evaluateAuditOutcome`, checked `localAgent` turn reason via `foldConsumedWork`, waited for parallel sibling audits in `runLoop`, and handled failure reporting with preserved reports.
- `root/.dsh/plugins/build-loop/test.mjs`: Added unit tests for structured verdict acceptance (including malformed report and findings elements), plain-text fallback under DSH outputSchema error conversion (with full verdict assertions), partial failed output rejection (including diagnostic preservation), empty execution rejection, three-attempt limit, retry success, thrown exceptions, caller cancellation (pre-abort, abort-with-throw, and post-attempt resolve cancellation), child disposal across attempts, failure formatting, and report preservation on audit failure.
- `root/.dsh/plugins/build-loop/README.md`: Updated flow documentation to describe audit child retry semantics, plain-text fallback, and failure reporting.
- `AGENTS.md`: Added concise invariant covering audit retries without consuming fix rounds, plain-text fallback, and failure reporting with preserved reports.
- `.scratch/dsh-0.1.2-rc.1-upgrade/issues/06-simplify-audit-retries.md`: Marked ticket resolved and recorded the answer report.

### Design decisions taken
- Checked `run.localAgent.session.snapshotEvents()` turn reason with `foldConsumedWork` to distinguish normal completion (`turnReason === 'completed'`) from actual execution failures when DSH's `outputSchema` maps missing structured output to `stopReason: 'error'`.
- Placed retry orchestration (`runWithRetries`), audit outcome evaluation (`evaluateAuditOutcome`), and failure message formatting (`formatAuditFailure`) into `loop.mjs` as pure functions without runtime dependencies, keeping them testable under the repo's pure-test and no-mocks constraints.
- Retained prior audit reports in `state.review` and `state.test` when an audit phase exhausts its retry attempts, updating only the phase that completed.

### Deviations from the ticket
- None.

### Not implementable here
- Live host-plugin execution in a running DSH session; host plugins are imported once at boot and require a DSH runtime restart, which is prohibited to avoid terminating the active session.

### Fix round 1
- Accepted and addressed all four test audit findings in `root/.dsh/plugins/build-loop/test.mjs`.
- Added test assertion for signal abort with normal attempt resolution, verifying `runWithRetries` rejects with `/build_ticket was cancelled/`.
- Added assertions in `toVerdict` tests covering non-string report and non-string findings elements.
- Deleted `resPlainPlainStringResult` helper and asserted `clean`, `findings`, and `report` directly on fallback results.
- Added assertion verifying `result.diagnostic` on failed execution is preserved in `cause`.
