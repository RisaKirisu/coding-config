# Lessons

- Keep reviews inside requested change scope. Do not treat pre-existing runtime state as a defect in an unused fork unless migration was explicitly requested.
- Do not promote user-controlled escape hatches, optional validation, concurrency races, multi-file rollback, or command-line exposure into findings without evidence that the specification requires those guarantees or the normal execution path breaks.
- Rank concrete execution blockers above architectural preferences. Verify severity by reproducing the ordinary user path.
- When a project already exposes a credential service, use that service instead of duplicating its storage format and writer. Surface credential-service failures to the UI rather than swallowing them.
- Give subagents exact, bounded scopes and required skills. Explicitly forbid them from launching subagents. Do not investigate delegated scope while it is running; continue separate work or wait.
- Do not add bypass lifecycle commands to avoid repeated setup. Make shared `create` and `start` operations state-aware no-ops, and gate dependency installation on package-manager state.
- Lifecycle fakes must isolate absolute guest paths before evaluating guest commands. The old fake evaluated `/tmp/devvm-daemon-dsh.pid` on the host and killed the DSH Runtime running the tests. Guard lifecycle test runs by verifying the hosting PID remains alive and unchanged.
