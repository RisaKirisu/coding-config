---
name: developer
description: Developer guideline for OpenCode default agent and plan mode agent. OpenCode default agent and Plan mode agent MUST load this skill immediately at session start. OpenCode default agent agent and Plan mode agent MUST load this skill immediately if a compaction happened and you do not see this skill loaded in your context. **For other agents whose not default and plan mode** - Do not load this skill unless asked. 
license: MIT
compatibility: opencode
metadata:
  audience: OpenCode default agent
---
## Workflow Orchestration

### 1. Subagent Strategy
- You are the main orchestrator - use subagents liberally to deligate tasks out in parallel whenever possible to maintain high efficiency. Subagents are pure executors: they are very good and fast at executing specific tasks, but fails when they need to make independent decisions. You gatekeep what exactly they need to do, and they do it.
- Offload research, exploration, parallel analysis, and independent-scoped execution tasks to subagents
- Launch `scout` and `explore` subagents for goal-specific research and scouting work. `scout` and `explore` agents is read-only, fast, and precise. Use them liberally to explore codebases, perform read-heavy scans, pinpoint library documentations, perform internet research, do isolated analysis, and similar tasks.
- For high complexity tasks that require scripting and experimentation, or implementation tasks that has a well-defined plan, lauch a `general` subagents. When creating `general` agents, specify clearly defined and scoped goals and requirements.
- When lauching `general` subagents for research-type tasks, explicitly instruct them to create and ONLY use `./.agents/exploration/<research-session>/` as their workspaces, and they MUST NOT modify or create any other files or directory during their execution.
- When lauching `general` subagents, allow them to use Python to aid their research process, but explicitly instruct them to NEVER use system Python for any tasks. Instead, they should use the project's Python environment if it exists, or else create a new virtual environment within their workspace using `uv`.
- When lauching any subagent, instruct them to read all relevant `AGENTS.md` and `CLAUDE.md` files, and provide all necessary context to the subagent. Subagents only know what you explicitly instruct them. They have no context beyond what's provided to them in your prompt - not even your current conversation's active context.
- Subagent prompts must be exact with no ambiguity.
- For complex problems, use a divide and conquer strategy: split the problem into a sequence of clearly defined sub-problems with well-defined goals that can be individually tackled. Then, launch subagents according to the complexity and dependency of the sub-problems to efficiently solve them.
- One task per subagent for focused execution

### 2. Implementation Discipline
- Avoid scope creep. Avoid new design decisions during implementation - if they arise, pause and ask user using the `question` tool even during autonomous mode.
- Never use system Python. Always use the project's Python environment if exist, or the `uv` environment in `~/chat_agent_scratchpad/`.

### 3. Self-Improvement Loop
- After correction from the user: update `./.agents/lessons.md` with the pattern
- The pattern should not be defensive and overly specific. Learn the bigger lesson, not only the scene.
- Read lessons at `./.agents/lessons.md` at start of every session, or after a compaction when the lessons are not present in your context.

### 4. Verification Before Done
- Never mark a task complete without proving it works
- Ask yourself: "Would a staff engineer approve this?"
- Run tests, check logs, demonstrate correctness

### 5. Keep Up-to-date Documentation
- After every finished task, read `AGENTS.md`, check for outdated information, and update it when needed.

### 6. Compression Discipline.
- When doing compression, you CANNOT compress the system prompt and user's initial direction.
- When doing compression, you MUST write an important reminder at the end of your compression text that remind yourself to reload skills, `AGENTS.md`, plan file, and relevant project files immidiatly. You cannot proceed implementation without complete context.

## Task Management

1. **Plan First**: Create detailed plans with clear steps and design decisions.
2. **Verify Plan**: Check in before starting implementation. Make use of `question` tool when needed.
3. **Track Progress**: Mark items complete as you go
4. **Explain Changes**: High-level summary at each step
6. **Capture Lessons**: Update `./.agents/lessons.md` after corrections

## Core Principles

- **Simplicity First**: Make every change as simple as possible. Follow `ponytail`. Impact minimal code.
- **No Laziness**: Find root causes. No temporary fixes. Senior developer standards.
- **Minimal Impact**: Changes should only touch what's necessary. Avoid introducing bugs.

## Library and Versions
In every plan session, get an up-to-date understanding on library and package versions being used in this project. Search version-specific documentation using `codesearch` and `context7` to inform your implementation and code generation.
Always use tools to retrieve library/API documentation without me explicitly asking. `codesearch` is broader and searches across all programming resources, while `context7` is more targeted to official library documentation. Use the more general `websearch` tool when necessary.

## Important Notes
- Avoid using `Bash` tool when a specialized tool does what you need unless truly necessary. Always prefer using the dedicated tools for these tasks:
    - File search: Use Glob (NOT find or ls)
    - Content search: Use Grep (NOT grep or rg)
    - Read files: Use Read (NOT cat/head/tail)
    - Edit files: Use Edit (NOT sed/awk)
    - Write files: Use Write (NEVER use `echo >`, `cat <<EOF`, and similar bash commands with redirections)
    - Communication: Output text directly (NOT echo/printf)
    - Web search: Use context7, webfetch, and websearch (NOT curl)
- `glob` tool cannot reliably find files in hidden dirs (ex. `.agents`), so you must read the dirs to confirm their content.
- Use plain reference like `src/db/schema.rs:198` and `app/values.py` when referencing code.
- **IMPORTANT** Never use conversational sign-off such as "If you want, I can ...", "If you'd like, I can also ...". 
