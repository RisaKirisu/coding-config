---
name: autonomous-building-protocol
description: Implement one defined, ready-for-agent issue as a builder subagent. Load when the caller says "autonomously build / impl" or hands you a ticket to implement.
---

## Build one issue

1. Read `AGENTS.md`, `CLAUDE.md`, `.agents/lessons.md`, the repo's issue-tracker doc, then the ticket. The ticket is the spec; the caller's prompt adds constraints. Set the ticket `Status: claimed`.
2. Load `tdd` and `ponytail`[full]. Read every file you will touch before editing. Plan actions.
3. Implement test-first. Test behavior, not implementation. Never edit a test to raise the pass rate; a failing test means the code is wrong until proven otherwise. Reward come after actual success implementation, not passed tests.
4. Verify: run the whole suite plus lint/clippy at the repo's strictness. `tee` output to a file and read the file; check exit codes. Clean up processes, files, and modes your tests or edits touched.
5. Mutation-test every new or changed test: disable, flip, or remove the production branch it protects; a named test must fail; restore. Breaking the assertion proves nothing. Presence checks pass when another code path emits the same text, so assert exactly-once, exact tag, exact order where it matters.
6. Self-review with `ponytail-review` and `code-review`: catch logic faults, missed requirements, over-engineering. Fix only findings in scope. At most two review-fix cycles, then stop and report what remains.
7. Resolve: set `Status: resolved`, tick verified acceptance boxes only, append `## Answer` (see report). Commit only if the repo has VCS and the caller asked; otherwise state that edits were irreversible and targeted.
8. Return the report.

## Report (ticket `## Answer` and final message, same content)

- Files changed, one line each.
- Test counts pass/fail/ignored; lint result.
- Mutants: each production mutation and the test that killed it.
- Deviations from the ticket, each with reason.
- Not verified here: every acceptance criterion this environment cannot exercise, and why.

## Rules

- Implement the ticket as written. Do not add, extend, or "improve" requirements. Do not touch unrelated code, tickets, or issues.
- When a requirement is impossible, contradictory, or clearly harmful, choose the smallest faithful alternative, record it under the ticket's `## Comments` and in Deviations. Never deviate silently. Never stop early to ask; you have no channel.
- Root causes only; no temporary fixes, no speculative flexibility, no history-narrating comments. Comments state invariants.
- No self-built mocks; use established libraries only, else state the gap.
- Edit with targeted replacements; never whole-file rewrite an existing file.
- Spawn subagents only if the caller permits, read-only, and forbidden from spawning further.
- If context is compressed mid-task, reload this skill, `tdd`, `ponytail`, the ticket, and the repo guidance before continuing.

[StopCondition] Ticket implemented as written, whole suite and lint green, every new test kills its mutant, ticket resolved with the report.