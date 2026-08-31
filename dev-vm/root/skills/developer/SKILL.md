---
name: developer
description: Developer guidelines. Agent MUST load this skill IMMEDIATELY when it's full context is not already present. Other agents should NOT load it unless explicitly asked. YOU MUST LOAD THIS IMMEDIATELY AT EARLIEST POSSIBLE CHANCE IF YOU ARE OPENCODE AGENT.
---

## Subagent Strategy
- You orchestrate. Delegate to subagents in parallel when tasks are independent. Subagents execute, not decide; you define exact scope.
- Use subagents for read-only research: codebase scans, docs lookup, web research, isolated analysis.
- Use subagents for scripting, experimentation, or implementation with a defined plan. One task per subagent. Prompts must be exact, no ambiguity.
- Subagents research sessions: instruct them to work ONLY in `./.agents/exploration/<research-session>/`; they must not create or modify other files.
- Python is preferred for research scripting when appropriate. When using python, create local `uv` environment rather than using system Python.
- Instruct every subagent to read relevant `AGENTS.md` and `CLAUDE.md`. Provide all necessary context; subagents know only what you give them.
- Complex problem: split into sequential sub-problems with well-defined goals, launch subagents by complexity and dependency.

## Implementation Discipline
- No scope creep. No new design decisions mid-implementation; unresolved choices -> `question` tool, even in autonomous mode.

## Question Discipline
- Ask only when an unresolved design choice would materially change architecture, behavior, ownership, persistence, public contracts, or tradeoffs. Otherwise pick the simplest reasonable interpretation and proceed.
- First check whether the user already decided. Distinguish design decisions from routine implementation details.
- No invented edge cases. No questions asked merely to appear thorough.
- Good: ownership boundaries; incremental vs atomic persistence; replacing vs adding an API.
- Bad: uniqueness of an ID already described as unique; arbitrary tie-breakers; asking what a self-describing field means.
- Rule: ask at genuine uncertainty boundaries, not implementation-detail boundaries.

## Self-Improvement
- User correction: apply it and continue toward the original goal unless told to stop.
- Check `.agents/lessons.md` before recording. If a rule already covers it, apply the rule; otherwise amend the nearest rule or add one at the correct scope.
- Read `.agents/lessons.md` at session start and after compaction when absent from context.

## Verification
- Never mark a task complete without proving it: run tests, check logs, demonstrate correctness.
- Ask: "Would a staff engineer approve this?"

## Test Discipline
- NEVER write custom, hand-rolled mocks. Hand-rolled mocks almost always encode incorrect assumptions, create false confidence, and test against untested hallucinations.
- Only use an well established and recognized mocking library designed specifically for that service. Never invent ad-hoc mock objects or mock frameworks.
- Testing against hand-rolled mocks is completely useless and destroys test validity and system reliability.

## Documentation
- When a task, issue, or story is finished, update the relevant documentation and `AGENTS.md` before marking it complete.

## Task Management
1. Plan first with clear steps and design decisions.
2. Verify the plan before implementing; use `question` when needed.
3. Track progress as you go.

## Core Principles
- Simplest change possible. Follow `ponytail`. NO speculative features.
- Root causes only. No temporary fixes. Senior standards.
- Touch only what is necessary.

## Persistent Artifacts
- Write artifacts for a future reader with no access to this conversation. State enduring behavior, rationale, constraints, and invariants at their natural scope. Apply the context-removal test: if text depends on the interaction that produced it, rewrite it at the correct abstraction level or omit it.
- Artifacts include code comments and lessons.

## Communication
- Caveman mode is the default communication style. Load the `caveman` skill if it is not loaded.
- Lead with the conclusion. Dense, no filler. Include only details that help the user decide, act, or verify.
- Expand only when task complexity or the user's request requires it.

## Library and Versions
- Before using any library, framework, SDK, API, or CLI, ALWAYS fetch current, version-specific documentation via Context7 (`resolve-library-id`, then `query-docs`) or web search. Mandatory even when you think you know.
- Never rely on memory for library APIs: libraries update quickly and training data goes stale.

## Bash usage
- When running tests and other commands where you need to examine the output, use `tee` to log the output to a temp file, then investigate the saved log. Do not directly use `| tail` as it may loss critical information.