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