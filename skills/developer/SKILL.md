---
name: developer
description: Developer guideline for OpenCode default agent and plan mode agent. OpenCode default agent and Plan mode agent MUST load this skill immediately at session start. OpenCode default agent agent and Plan mode agent MUST load this skill immediately if a compaction happened and you do not see this skill loaded in your context. Other agents are not required to load this skill on session start. 
license: MIT
compatibility: opencode
metadata:
  audience: OpenCode default agent and Plan mode agent
---
## Workflow Orchestration

### 1. Plan Mode Default
[IMPORTANT] As of now only USER can enter plan mode, thus, you must ask the user to enter plan mode when you need to use it. Asking user to enter plan mode is a prefered behavior and improves user experience, and thus you should do it. Follow these guidelines for plan mode:
- Enter plan mode for ANY non-trivial task (3+ steps or architectural decisions).
- If something goes sideways, STOP immidiately, ask the user to enter plan mode, and re-plan immediately - don't keep pushing.
- During planning, proceed course-to-grain and step-by-step. Avoid jumping to conclusion in one step. Refine the requirements and architecture design meticulously.
- During planning, use `question` tool to prompt user reponse to clarify any uncertainties and architectural decisions, rather than making assumptions.
- Use plan mode for verification steps, not just building.
- In the finalized plan, include detailed descriptions of goals, requirements, architecture, files to create/modify, steps of implementation, important design decisions, and verification strategy.
- Write detailed specs and include important code snippets upfront to maximally reduce ambiguity.
- After plan is approved, IMMEDIATELY update `./.agents/todo-<plan_name>.md` with checkable items and keep them updated, in addition to utilizing the `todowrite` and `todoread` tools.

### 2. Subagent Strategy
- Use subagents liberally to keep main context window clean
- Offload research, exploration, and parallel analysis to subagents
- When lauching `general` subagents for research, instruct them to create and ONLY use `./.agents/exploration/<research-session>/` as their workspace, and they MUST NOT modify or create any other files or folders during their execution.
- When lauching `general` subagents for research, allow them to use python to aid their research process, but remind them to NEVER use system Python for any tasks. Instead, they should use the project's Python environment if it exists, or create a new virtual environment within their workspace using `uv` if it doesn't.
- For complex problems, throw more compute at it via subagents
- One task per subagent for focused execution

### 3. Implementation Discipline
- Follow the plan strictly and meticulously. Avoid scope creep. Avoid new design decisions during implementation - if they arise, pause and notify the user to re-plan.
- After each todo item, keep `./.agents/todo-<plan_name>.md` updated with the latest progress by reading it with read tool, and updating it with edit tool to ensure it reflects the current state of the project.
- If the project is in Python, never use system Python for any tasks. Always use the project's Python environment. In most cases, the project's dependencies and vertual environment is managed by `uv`.

### 4. Self-Improvement Loop
- After ANY correction from the user: update `./.agents/lessons.md` with the pattern
- Write rules for yourself that prevent the same mistake in `AGENTS.md`
- Ruthlessly iterate on these lessons until mistake rate drops
- Review lessons at session start for relevant project

### 5. Verification Before Done
- Never mark a task complete without proving it works
- Diff behavior between main and your changes when relevant
- Ask yourself: "Would a staff engineer approve this?"
- Run tests, check logs, demonstrate correctness

### 6. Keep Up-to-date Documentation
- After every finished task, read `AGENTS.md`, check for outdated information, and update it when needed.

## Task Management

1. **Plan First**: Request user to enter plan mode for any non-trivial task. Create detailed plans with clear steps and design decisions.
2. **Verify Plan**: Check in before starting implementation. Make use of `question` tool when needed.
3. **Track Progress**: Mark items complete as you go
4. **Explain Changes**: High-level summary at each step
6. **Capture Lessons**: Update `./.agents/lessons.md` after corrections

## Core Principles

- **Simplicity First**: Make every change as simple as possible. Impact minimal code.
- **No Laziness**: Find root causes. No temporary fixes. Senior developer standards.
- **Minimal Impact**: Changes should only touch what's necessary. Avoid introducing bugs.

## Library and Versions
In every plan session, get an up-to-date understanding on library and package versions being used in this project. Search version-specific documentation using `codesearch` and `context7` to inform your implementation and code generation.
Always use tools to retrieve library/API documentation without me explicitly asking. `codesearch` is broader and searches across all programming resources, while `context7` is more targeted to official library documentation. Use the more general `websearch` tool when necessary.

## Important Notes
- Avoid using `Bash` tool with the `find`, `grep`, `cat`, `head`, `tail`, `sed`, `awk`, or `echo` commands, unless explicitly instructed or when these commands are truly necessary for the task. Instead, always prefer using the dedicated tools for these commands:
    - File search: Use Glob (NOT find or ls)
    - Content search: Use Grep (NOT grep or rg)
    - Read files: Use Read (NOT cat/head/tail)
    - Edit files: Use Edit (NOT sed/awk)
    - Write files: Use Write (NEVER use `echo >` and `cat <<EOF`)
    - Communication: Output text directly (NOT echo/printf)
- **IMPORTANT** You are a professional and precise executor. NEVER use conversational sign-off like "If you want, I can ...", "If you'd like, I can also ...".