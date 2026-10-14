# 06: Builds and tests on Windows

**What to build:** `devvm` compiles and its test suite passes on Windows x86_64, with Windows process handling that never hangs on output held by detached children and cancels commands as on Unix. No Windows DevVM features yet.

**Blocked by:** 05

**Status:** ready-for-agent

- [ ] On Windows, `devvm` clears inheritance on its own standard handles at startup, and the command runner captures output through files read after exit.
- [ ] Cancellation on Windows snapshots the command's descendants while it runs, terminates the command and its descendants except Smolvm VM processes (with their subtrees) and processes whose command line cannot be read, and waits only for the terminated ones; Unix keeps process-group SIGTERM.
- [ ] Test helpers build exit statuses on both platforms; Unix-only tests (file modes, process groups, the Linux browser fixture, the bash-based DSH probe test) are gated to Unix.
- [ ] Windows tests with real processes: a captured command whose child keeps running returns; after cancellation the command and a running descendant are gone and a preserved process remains.
- [ ] Tool lookups in tests are cross-platform; the live test's unused host rsync requirement is removed.
- [ ] CI adds a Windows job running the tests, and it passes.
