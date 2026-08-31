# 01: Build the Control Daemon and manage Project runtimes

**What to build:** Deliver the complete local management path: preserve the existing DevVM CLI, add the Rust Control Daemon and embedded UI, register Projects, and manage DevVM and DSH Runtime lifecycles with inspectable Project Logs.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Existing DevVM shell, start, stop, execute, status, and removal workflows remain usable without the Control Daemon.
- [x] The Control Daemon can run in the foreground and serve its embedded vanilla HTML/JavaScript UI locally.
- [x] The Project Browser lists directories beneath the daemon user's home and cannot browse outside that root.
- [x] Registering a Project creates a UUID Project ID in `.devvm-id` when absent and reuses a valid existing Project ID.
- [x] Registered Projects persist in a small local registry and appear in the central UI with DevVM and DSH Runtime status shown separately.
- [x] Every Project is mounted at `/root/workspace` inside its DevVM.
- [x] Portable DSH State remains on the DevVM filesystem while workstation-wide DSH credentials, plugins, profiles, skills, presets, and settings remain centrally shared.
- [x] The UI starts and stops DevVMs and can delete a local DevVM explicitly.
- [x] Unregister removes only the Project registry entry and does not delete the local DevVM or synchronized data.
- [x] Launch DSH starts the DevVM when needed, starts DSH once, reports DSH status independently, and provides a browser link when running.
- [x] Unexpected DSH exit is reported as failed, with manual restart rather than automatic restart.
- [x] DSH/plugin output and Control Daemon command output are captured as host-persisted Project Logs and recent logs are visible in the central UI.
- [x] The central HTTP interface exposes the agreed lifecycle operations and does not expose generic command execution.
- [x] Automated tests exercise the real Control Daemon HTTP interface with temporary state and a fake DevVM CLI.
