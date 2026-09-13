# Build-ticket process router

The plugin routes a builder and two auditors, pauses for caller decisions, and tracks completion of the assigned work. It does not inspect Git, compute hashes, scan repository files, sandbox workers, or write filesystem checkpoints. A normal directory is sufficient.

## Process

`build_ticket` starts a run with a ticket and contract. The builder proposes an approach and stops. The caller approves or revises it through `build_ticket_decide`. The builder then implements and runs the approved checks; a failing check returns to the builder for at most two retries. Both auditors report to the caller. The caller triages every open finding with `fix` or `ignore` and a reason. Only approved fixes reach the builder; each new batch consumes one fix round (`maxFixRounds`). A builder may dispute a fix or ask for guidance at any time. This pauses the run without closing the finding. Resuming the same approved batch does not consume another round.

Runs live only in memory. `run_id` selects a run, and `revision` is a decision counter that rejects stale or overlapping calls. It is not a source version. An interrupted operation can resume while this plugin instance lives; host restart or plugin reload loses the run. There are no run files or recovery snapshots. Existing DSH conversation history and settings remain managed by DSH.

## Contract and decisions

The contract contains `summary`, `scope`, `behaviors`, and `checks`. Each behavior has an ID, observation, and optional check ID. Each check has an ID, command, scope, and expected exit code. Scope is an instruction for workers and caller review; the controller does not enforce it against the filesystem. Behavior IDs are labels for workers and the caller; the controller does not match observations or confirmations against them. The code auditor also covers `@maintainability`.

Decision kinds: `approve_design`, `revise_design`, `triage`, `continue`, `ask`, `accept`, `abandon`, and `inspect`. Triage supplies `dispositions: [{id, action: "fix" | "ignore", reason}]` for every open finding. An all-ignore decision needs no further model round when verification is already complete. `continue` resumes an already approved task with guidance; it cannot approve untriaged fixes or bypass the fix-round cap. `inspect` reads the current run without executing work. No new workflow phase is introduced.

Checks captured during a builder attempt count for that attempt. After each new handoff the controller runs any approved check without a passing result for the current attempt.

## Worker execution

An implementing builder uses `build_ticket_check` to run an approved command through the existing bash tool. Results include exit status, duration, stdout, and stderr. The plugin does not create separate log files or a custom shell executor. Native runtime output spill files, execution policies, and cancellation continue to work normally.

One builder per run starts on first use and stays alive through failures until accept or abandon; every later builder turn is a follow-up to that same agent. Auditors receive the assigned obligations and relevant findings, not the other auditor’s private report. They are instructed not to edit source. The plugin adds no filesystem permission layer. Denied tools that a child inherits from the preset or global layer are hidden through the child's tool filter; a tool installed on each agent's own layer (the Web `subagent` tool) cannot be filtered and is instead refused at execution by a global guard keyed on the worker's session. Owned background jobs are collected or stopped before a worker handoff, and a failing audit cancels and drains its sibling.

Acceptance checks process prerequisites: ready handoff, successful checks for the current attempt, completed code/test audit obligations, no unresolved infrastructure failure, and no open findings. Disputed findings remain open and block acceptance. Only a caller ignore decision or an auditor resolution removes that blocker. Checks and audits must belong to the current builder attempt, so ignoring a dispute does not skip unfinished verification. Source correctness and scope compliance remain caller judgments.

## Prompts

Worker prompts are the files under `instructions/`, read by `prompts.mjs` when a run starts; a run pins the text it started with, and the next run picks up file edits without a host restart. The builder persona is `common.md` plus `builder.md`. Each auditor persona is `common.md`, its role file (`code-reviewer.md` or `test-auditor.md`), and `auditor-findings.md`, which defines the shared internal 0–100 confidence rubric: only scores strictly greater than 75 qualify, reports carry impact and evidence rather than a confidence field, a material unverified premise caps confidence at 75, and impact does not raise confidence. Prompts are not stored in settings and have no override.

Each auditor receives its own ignored findings and caller reasons. Silence leaves them ignored. Stronger evidence may reopen an ignored ID through `prior: [{id, status: "open", evidence}]`; the same confidence threshold applies. The controller validates IDs and report shape, not evidence quality. Reopened findings return to caller triage. Reuse IDs rather than creating duplicate findings.

The persona enters a worker once, as its system prompt. Each builder turn sends only the current assignment and instruction; each audit sends the assignment packet and the audit assignment. The `agent/pre-step` hook injects a `<system_reminder>` with the builder persona and current assignment only after compaction and whenever context pressure has grown by `reminderTokens` (default 100K) since the previous reminder. Pressure is `ctx.tokenMeter.measure(session).totalTokens`, the host token meter's replay measurement anchored on provider-reported usage, so it tracks the billed context size rather than a string-length estimate. The first step of a fresh builder may run before the worker is registered and is not measured; nothing is lost because pressure is far below the threshold there. A malformed JSON report receives one same-worker format-only repair; quota or execution failures return to the caller.

## Settings and verification

The settings page and `/api/build-loop/config` hold the flow settings only: `provider`, `maxFixRounds`, `reminderTokens`, `deniedTools`. Settings persistence belongs to DSH, independently of in-memory run state. Changing settings affects new runs. Source changes require the host to reload the plugin; editing files does not change an already loaded instance.

Run `node --test /root/.dsh/plugins/build-loop/test.mjs`. Tests use real DSH tool dispatch, agents, and shell processes in plain temporary directories without Git. No model provider is installed in the test host, so these checks cover process routing and failures rather than successful model-generated code.
