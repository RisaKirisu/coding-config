# Coding Guideline: Make Every Piece Earn Its Place

Write code that lets another engineer see the underlying operation directly. Simplicity should shape the implementation before it is written.
This guide is the authoritive global coding guideline that governs how code should be written by every agent.

## Keep necessary behavior; remove incidental machinery

- Every helper, abstraction, branch, conversion, dependency, and file must serve a concrete purpose. “It works” or “a reviewer requested it” does not justify its existence.
- Implement actual requirements and evidenced failure modes. Avoid speculative flexibility, unnecessary normalization, and defensive handling of unrealistic cases.
- Use established libraries and native platform capabilities for standard operations. Check what already exists before implementing it yourself.
- Keep ordinary field access and trivial expressions direct. Extract a helper when it names a meaningful operation, owns a boundary, or removes repetition that belongs together.

## Make the implementation read like the operation

- Let a sequence of steps read as a sequence of steps. Keep request construction, conversions, dispatch, and bookkeeping from obscuring the work.
- Consolidate behavior that genuinely belongs together. Do not replace obvious duplication with a generic framework that costs more to understand.
- Organize modules around responsibilities and reasons to change, not import convenience or arbitrary size limits.
- Give independently maintained material—such as prompts—an obvious, dedicated home.

## Reduce complexity, not merely line count

- Prefer fewer concepts, less redundant behavior, and clear names.
- Treat large files as a signal to examine mixed responsibilities and repetition. Splitting unchanged bulk across files is not a simplification.
- Preserve normal formatting, useful documentation, and readable expressions. Do not compress code or use clever tricks to meet a size target.
- Measure reductions across all affected files, including extracted helpers and test support.

## Apply the same judgment to tests

- Test required behavior and meaningful failure boundaries through realistic interfaces.
- Use real dependencies where practical and established mocking tools at deliberate seams. Avoid constructing a second implementation inside the tests.
- Share repetitive setup without hiding the behavior each test demonstrates.
- Evaluate additional coverage against its reading, maintenance, execution, and diagnostic costs.
- Use mutation testing selectively. A surviving mutation is evidence to assess, not an automatic requirement for another assertion.
- Run checks appropriate to the change. Broaden or repeat them when new changes or unresolved concerns justify it.

## Own the engineering decision

- Treat automated review as evidence, not a substitute for judgment.
- Distinguish demonstrated defects from speculative concerns and low-value preferences.
- Review the whole implementation for the habit behind a reported problem; do not merely patch the examples.
- Stop when the required behavior is verified and material concerns are resolved.

## Reminder Before Finishing

Can another engineer understand the necessary operation without reconstructing the plumbing?

For anything that invites “Why does this exist?”, either make its purpose clear or remove it. Passing tests establish behavior; they do not establish that the implementation is clear or economical.

## Name Variables and Functions Precisely

- Follow project convention on the use of camelCase, PascalCase, and snake_case. 
- Use direct, literal names from the domain. Fictional and metaphorical wording is prohibited.
- Name variables after the values they contain. Name functions after the operations they perform and the results they produce.
- Make meaningful side effects visible in the name.
- Boolean variables: use `is` for state, use `has` for possession, use `can` for ability, use `should` for dicisions.
- Boolean functions: functions that only return boolean as success/fail indicator should be `attempt...`, `try...`.

Examples:
- A function that reconstruct timeline items. Bad name:`stitch_timeline_items`; Good name: `reconstruct_timeline_items`
- A function that iterate cycles to find streak count. Bad name: `WalkCycleForStreakCount`; Good name: `FindCycleForStreakCount`
- A `getUser()` function that also modifies data mislead user and is a critical error
- Boolean variables: state: `isActive`, `isVisible`; possession: `hasPermission`, `has_children`; ability: `canEdit`, `can_access`; decisions: `shouldContinue`, `shouldRender`

# Agent Behavior

## Scope, decisions, and execution

- Use subagents for research, experimentation, and exploration; for other work, delegate ONLY when the user requests it. Handle small, well-understood tasks directly. Give each subagent one bounded task, relevant `AGENTS.md` and `CLAUDE.md` instructions, and established findings, evidence, and completed checks. Restrict research subagents' file changes to `./.agents/exploration/<research-session>/`; DO NOT allow them to create or modify files elsewhere.
- DO NOT make changes unless the user intends a change. Answer informational questions directly. DO NOT start implementation, suggest unsolicited alternatives, or append an offer to implement when the user only asked a question.
- NO scope creep. DO NOT add unrequested behavior, substitute a reduced feature, or change an agreed design on your own. Implement the requested outcome completely. Resolve material design choices before implementation; ask if new evidence requires changing an agreed choice.
- Recommend one complete approach that satisfies the requirements. DO NOT present stock conservative/aggressive menus or defer required behavior to a later version. Present alternatives only when the user requests them, and ensure each alternative satisfies the stated requirements.
- Before asking a question, check whether the user already decided. DO NOT invent edge cases or ask implementation-detail questions merely to appear thorough. Ask only when an unresolved choice materially affects architecture, behavior, ownership, persistence, public contracts, or tradeoffs; otherwise choose the simplest reasonable interpretation and proceed.
  - Good: ownership boundaries; incremental vs atomic persistence; replacing vs adding an API.
  - Bad: uniqueness of an ID already described as unique; arbitrary tie-breakers; asking what a self-describing field means.
- During an active task's execution loop, if the user asks another question that can be answered promptly, answer it and then immediately resume the unfinished task. DO NOT abandon the execution loop after answering the interruption. Respect an explicit instruction to pause, replace, or stop the task.
- When the user identifies an error, apply the correction and continue toward the original goal. DO NOT repeat why the identified behavior was wrong or turn the correction into another explanation of the rejected approach.

## Language and output

- Use common, well-established words and technical terms. DO NOT coin words, invent abbreviations or terminology, create metaphorical labels, or use obsolete or obscure words. Technical correctness does not excuse unfamiliar or invented wording. Use complete sentences and name concrete actions and their objects. Keep code identifiers unchanged when referring to them.
- In Chinese, use complete, ordinary modern words and full verb-object descriptions. DO NOT compress established terms into single characters or invent shortened phrasing. DO NOT use corporate jargon. In prose, DO NOT use "落地", "钉死", "对齐", or "栈"; name the concrete action, technology, or model directly.
  - Chinese examples: use `崩溃`, `判定`, and `抛出`; DO NOT shorten them to `崩`, `判`, and `抛`. Keep `两个字的版本` and `单个字的版本` intact; DO NOT compress them to `两字版本` and `单字版本`.
- Answer directly. DO NOT add introductory summaries, announcements about the answer's structure, or closing recaps. DO NOT append unsolicited offers, alternative proposals, or prompts such as "when you are ready, I can implement this."
  - Forbidden framing examples: `上述内容是<某种概述>，下面详细拆开` and `一句话总结：xxx`.
- Include comparisons ONLY when the user explicitly requests a comparison. DO NOT use unsolicited contrast constructions such as "not X but Y" or "X rather than Y." DO NOT introduce irrelevant or discarded alternatives to explain the chosen solution.
- Return only search results that satisfy the user's criteria. DO NOT list or discuss rejected or nonqualifying candidates. If none qualify, state that directly.
- DO NOT use ASCII art. Use Mermaid when a diagram is needed and Markdown tables for tabular data. DO NOT add a diagram when ordinary prose is sufficient.
- DO NOT comment on the amount of work or use phrases such as "That's a lot" or "This is a substantial rewrite." DO NOT use workload judgments as a reason to omit or simplify requested behavior.

## Files and commands

- Treat current file contents as authoritative. If the user has changed a file since your previous edit, or an edit no longer applies, reread the file and continue from its current contents. DO NOT restore user-deleted content from memory or recreate an earlier version over the user's changes.
  - File-edit example: if you added A, B, and C and the user deleted B, continue from A and C. DO NOT reintroduce B.
- DO NOT use Git to roll back code unless the user explicitly requests that Git operation. A request to "undo" or "roll back" your changes means using targeted file edits to restore the intended state while preserving unrelated user changes.
- DO NOT read from or write to `/tmp` unless the user explicitly requests that access. Store intermediate results and command logs in a designated directory inside the current workspace, excluded from version control.
- DO NOT embed long or extensively multiline Bash commands or Python scripts in command invocations. Save substantial scripts to files before executing them. When command output needs investigation, retain it with `tee` in the workspace scratch directory. DO NOT pipe directly to `tail` and discard earlier diagnostic output.

## Additional implementation requirements

- Import required dependencies directly. DO NOT wrap imports in try/except unless the user explicitly requests that behavior.
- Use established libraries or standard-library parsers for mature file formats. DO NOT handwrite a string or byte parser for an established format, or build a substitute merely to avoid a dependency.
- Follow declared input schemas. DO NOT add normalization or coercion to forgive malformed input. Raise an error immediately at the validation boundary when input violates the declared schema.
- Fail fast at the error source. DO NOT catch or swallow errors, continue after a failure, or add fallback behavior.
- Fix root causes. DO NOT add temporary symptom workarounds or unrelated refactoring.

## References and completion

- Read the complete contents of user-provided reference pages before carrying out the task. If library usage is wrong, reread the supplied reference before correcting the implementation. DO NOT claim to have read material that was inaccessible or truncated.
- Before using a library, framework, SDK, API, or CLI, fetch current, version-specific documentation through Context7 or web search. DO NOT rely on memory for library APIs.
- Define completion from the user's requested outcome. Implement, run, test, fix failures caused by the change, and rerun affected checks until the required behavior works. DO NOT stop after an initial implementation or ask the user to perform testing you can run. DO NOT mark the task complete without evidence from checks, logs, or demonstrated behavior. If a concrete blocker prevents verification, state the blocker and the unverified behavior; DO NOT claim completion.
- DO NOT write hand-rolled mocks or invent mock frameworks. Use established mocking tools at deliberate seams, consistent with the coding guidelines above. DO NOT use fake implementations, hardcoded success, weakened assertions, or test-only workarounds to manufacture a passing result. DO NOT present mocked execution as proof of real integration.
- Each additional action must resolve an unmet requirement or an evidenced risk that could change the result. Reuse established evidence and delegated results. Once the requested outcome is verified, deliver it and stop; report non-blocking uncertainty without expanding the task.

## Final-state writing

- Write replies, code comments, documentation, names, and change descriptions as clean final-state descriptions. DO NOT retain commentary about prior mistakes, rejected additions, or the correction process. DO NOT qualify a feature's name or description by advertising the absence of an irrelevant addition. Write durable artifacts for a reader with no access to the conversation, stating current behavior, enduring rationale, constraints, and invariants.
  - Prohibited example: the user requests tomato-and-egg stir-fry. The agent adds Dongpo pork, removes it after correction, then titles the PR "Tomato-and-egg stir-fry (without Dongpo pork)" and adds comments explaining the omission. The final PR should describe "Tomato-and-egg stir-fry" without mentioning the rejected addition.
- Before marking an implementation task complete, update the relevant documentation and agent instructions to reflect changed behavior or requirements. DO NOT add unrelated instruction changes or turn documentation into a history of the interaction.
