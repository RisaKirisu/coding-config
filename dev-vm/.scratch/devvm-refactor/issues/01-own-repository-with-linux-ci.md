# 01: Own repository with Linux CI

**What to build:** DevVM lives in its own private GitHub repository, `RisaKirisu/devvm`, extracted from the personal configuration repository's DevVM subdirectory with its full history. Every push and pull request runs format, lint, and test checks on Linux. The spec and plan for this work are in `spec.md` and `plan.md` beside this directory.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

Before starting, the maintainer creates the empty private repository on GitHub and logs this DevVM in with a token limited to that repository (see `context.md`). This ticket runs in the `coding-config` Project's DevVM, where the history to extract lives.

- [ ] The planning files under the DevVM subdirectory's `.scratch/devvm-refactor/` are committed to `coding-config` before extraction.
- [ ] The extraction runs on a fresh local clone of `coding-config` inside a gitignored directory of the workspace, not on the workspace repository itself.
- [ ] The new repository contains only the former DevVM subdirectory, at its root, with its commit history.
- [ ] A `ci` workflow runs on pushes and pull requests on the Linux hosted runner: format check, lints treated as errors, and the test suite; it passes on the main branch.
- [ ] Line endings are forced to LF for the Dockerfile, scripts, patches, the shared root, shell scripts, and TOML files.
- [ ] Repository paths in `AGENTS.md` and the readme name the new repository.
- [ ] The DevVM subdirectory remains in the personal configuration repository; it is removed only after the clean reinstall in ticket 03.
