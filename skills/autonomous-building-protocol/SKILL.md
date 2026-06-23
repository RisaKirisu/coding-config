---
name: autonomous-building-protocol
description: Implement defined ready-for-agent issues autonomously in a loop. Load when user say "autonomously build / impl". Reload after every compression when you are in an autonomous building session. 
---

## Impl issues autonomously in a loop
Issue impl loop:
0. If a compression just happened, rebuild context, including this skill, other loaded skills, and issue ticket. 
1. Load TDD skill, issue ticket, and plan for actions
2. Implement issue.
3. Compress implementation session.
4. Load review skill and perform code review.
5. Determine the findings worth fixing. These are findings about logic fault or missed requirements. Sometimes review surfaces invalid/unimportant findings.
6. Plan and Implement review finding fixes.
7. Compress.
8. Review again.
9. Repeat review -> fix -> compress until requirements are satisfied and bug-free. Do not get trapped.
10. Mark issue as complete. git add + git commit current work with commit msg describing work. Commit msg start with `oc: `
11. After commit, compress the session into concise summary -- if previous summaries exist, compress them as well.
12. Do NOT stop. Start next issue.

[StopCondition] ALL issues in target user-requested issue group are implmented, tested, and fully working.

## Important Notes:
- Finish given issues autonomously.
- Do not over-reach into un-requested issues.
- NEVER create new issues on your own.
- Create progress tracker under `.scratch/autonomous-progress/- <task>-progress.md`.
- Append progress entry with iso format local time after implementation, review, or fix.
- Each entry is short, current progress only. No detailed notes.
- Launch `explore` subagents to perform code, reference, or docs - researches in parallel. They can involve web search, context7, or repo research.
- When performing all compression and compaction, always list the context and skills to reload in the compaction summary. 