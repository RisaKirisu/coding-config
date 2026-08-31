# 05: Verify the complete version-one system

**What to build:** Prove the complete version-one workflow across the approved automated seams and an opt-in real-system acceptance run.

**Blocked by:** 04: Install the complete system on Linux and macOS

**Status:** resolved

- [x] The Control Daemon HTTP seam verifies Project browsing, registration, DevVM and DSH Runtime lifecycle, Open Port, Project Logs, Sync Status, Unregister, and separated deletion behavior.
- [x] The DSH plugin seam verifies manual Session Sync, automatic sync after completed turns and relevant saved changes, save ordering, scheduling, retries, status, and startup reconciliation.
- [x] The ingress seam verifies local and tailnet Project URL routing plus Loopback Facade behavior using real Caddy.
- [x] The DNS seam verifies wildcard private-name resolution using the real DNS mode.
- [x] FRP configuration and transport pass the agreed focused smoke test.
- [x] An opt-in real-system acceptance run exercises SmolVM, DSH, FRP, Caddy, rsync/SSH, DNS, and Tailscale together.
- [x] Real-system acceptance is completed on Linux and macOS.
- [x] The demonstrated workflow covers registering a Project, launching its DevVM and DSH Runtime, opening DSH and an arbitrary guest port locally and through Tailscale, observing logs, synchronizing Portable DSH State, and restoring it on another workstation.
- [x] The complete workflow remains local-and-tailnet only and is not exposed to the public Internet.
