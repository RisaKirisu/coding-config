# Lessons

- Keep reviews inside requested change scope. Do not treat pre-existing runtime state as a defect in an unused fork unless migration was explicitly requested.
- Do not promote user-controlled escape hatches, optional validation, concurrency races, multi-file rollback, or command-line exposure into findings without evidence that the specification requires those guarantees or the normal execution path breaks.
- Rank concrete execution blockers above architectural preferences. Verify severity by reproducing the ordinary user path.
- When a project already exposes a credential service, use that service instead of duplicating its storage format and writer. Surface credential-service failures to the UI rather than swallowing them.
- Give subagents exact, bounded scopes and required skills. Explicitly forbid them from launching subagents. Do not investigate delegated scope while it is running; continue separate work or wait.
- Do not add bypass lifecycle commands to avoid repeated setup. Make shared `create` and `start` operations state-aware no-ops, and gate dependency installation on package-manager state.
- Lifecycle fakes must isolate absolute guest paths before evaluating guest commands. The old fake evaluated `/tmp/devvm-daemon-dsh.pid` on the host and killed the DSH Runtime running the tests. Guard lifecycle test runs by verifying the hosting PID remains alive and unchanged.
- The web DSH profile installs local `file:` plugins as pnpm hardlink copies; editing the source breaks the links and leaves the installed copy stale. Refresh the profile install after editing a plugin before trusting a boot test.
- Functions that read process-wide env vars (`DEVVM_ROOT`) race under parallel tests; serialize those tests and point the env var into the temp tree so provisioning never writes into the real home.
- Never rewrite executables in this repo with the `write` tool; after any edit to `devvm` or `scripts/*`, re-check the mode with `ls -l`.
- Builder "non-vacuity" claims are not verification. Mutate the production code the test is meant to protect (disable the branch, flip the tag, drop the sort) and confirm a test fails; a test that only checks presence passes when a second code path produces the same text.
- A tailnet DNS acceptance test must traverse installed DNS service and tailnet resolver path. Binding a random loopback UDP port and querying it directly proves only packet parsing, not remote Project URL resolution.
- Inside the DevVM, `dsh plugin --profile web add|install` needs `--store-dir /root/workspace/.pnpm-store/v11`; the profile `node_modules` was linked from that store and pnpm refuses the default `~/.dsh/.pnpm-store`. Use `link:` specs for local plugins (AGENTS.md). Under `nodeLinker: hoisted` even `link:` materializes hardlinked copies, so after editing a plugin source run `rm -rf profiles/web/node_modules/<pkg> && dsh plugin --profile web install ...` before restarting.
