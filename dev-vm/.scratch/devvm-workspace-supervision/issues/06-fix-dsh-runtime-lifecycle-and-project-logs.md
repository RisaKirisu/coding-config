# 06: Fix DSH Runtime lifecycle reliability and Project Log readability

**What to build:** Make Control Daemon DSH Runtime start, stop, retry, and log inspection reliable under non-interactive DevVM execution.

**Blocked by:** None

**Status:** resolved

- [x] Routine DevVM startup skips DSH profile dependency installation when the profile lockfile already matches the installed dependency lock.
- [x] Required profile installation runs non-interactively and cannot fail because pnpm lacks a TTY.
- [x] DevVM creation and startup are state-aware no-ops when the machine already exists or is already running, so ordinary execution and DSH stop do not rerun startup preparation or mutate the active DSH profile.
- [x] DSH Runtime startup has a bounded readiness deadline, reports failure instead of remaining in `starting`, and permits a later manual retry.
- [x] Control Daemon child processes receive closed standard input rather than inheriting an unusable service input stream.
- [x] Concurrent multiline Project Log appends are written as one entry, log tails begin on complete lines, and ingress lines retain their original timestamps.
- [x] The Project Log modal refreshes automatically while open and stops polling when closed.
- [x] Regression tests cover non-TTY profile setup, state-aware creation and startup, Stop during startup, ordinary stop execution, startup timeout and retry, complete log tails, ingress formatting, and log-modal refresh through public CLI and HTTP seams.
- [x] The fake DevVM isolates its PID file from the host, so lifecycle tests cannot stop the DSH Runtime running the test suite.

## Root cause

`devvm start` and every `devvm exec` invocation reran `dsh plugin --profile web install --frozen-lockfile`. Control Daemon operations are non-interactive, so pnpm aborted when it wanted confirmation before rebuilding `node_modules`. The DSH stop path used the same prepared `exec` flow, causing dependency installation to run while DSH was still active. Separately, the runtime monitor had no readiness deadline, so a live child that never emitted the exact readiness line left the API request and runtime state in `starting` indefinitely. Project Log writes used multiple writes per entry, tails began at arbitrary byte offsets, ingress timestamps were replaced at read time, and the UI fetched logs only once.

The test suite also stopped its own hosting DSH Runtime because the fake DevVM evaluated the production stop command without translating `/tmp/devvm-daemon-dsh.pid`; it read the host PID file and killed the real process. The fake now intercepts guest stop commands and uses project-local process state.
