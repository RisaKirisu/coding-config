# 03: Single devvm binary drives DevVMs on Smolvm 1.27.3

**What to build:** One `devvm` binary replaces the bash `devvm` and the daemon binary on Linux and macOS. The Project commands and the Control Daemon run DevVMs in-process through the pinned Smolvm 1.27.3, with unchanged guest behavior. Ends with the maintainer's clean reinstall. Details are in the spec's Implementation Decisions (Smolvm, identity, guest setup, frps).

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] The binary is named `devvm`: `setup`, `serve`, `service`, `sync setup`, and the Project commands `start`, `shell` (default), `exec`, `stop`, `status`, `memory`, `rm`, `name`.
- [ ] `start` creates a missing DevVM with the configured CPUs, memory, storage, virtio-net, working directory, Project host variable, and the two mounts, starts it when not running, and runs guest setup only when a start happened.
- [ ] The guest setup drops the migration of old DSH layouts and fails the start when DSH or a profile manifest is missing; the profile lockfile check remains.
- [ ] DevVM state comes from one `machine ls --json` per status refresh (`running` is Running; `created`/`stopped` are Stopped; anything else is Failed); `memory` reports host, guest, and configured memory with null for unavailable values.
- [ ] The Control Daemon and the Project commands share one identity rule (the daemon's current Project host rule; machine name `dev-<name>-<hash>`), so Project URLs route for directory names containing repeated hyphens.
- [ ] Host frps is reused when a current-user process with the same executable and arguments runs, otherwise started detached with its log appended.
- [ ] The bash `devvm`, the setup script, and the image build script are deleted; the readme documents building a local image and selecting it in the configuration.
- [ ] The mocked test suites assert Smolvm commands instead of `devvm` commands; the failed-spawn observability test targets a non-executable Smolvm; the live acceptance test runs the new binary.
- [ ] CI adds a macOS job running the tests.
- [ ] The readme documents the clean reinstall procedure (Sync Status check, delete with the old tools, remove old Smolvm state, install, setup, re-register Projects).
- [ ] The readme, `AGENTS.md`, and the remote-access and runtime-maintenance docs describe the new commands.
- [ ] Maintainer, on the Linux workstation and the Mac: clean reinstall done; the live acceptance test passes; a DevVM survives two stop/start cycles; shares, bind mounts, writable `memory.reclaim`, and `fstrim` work in the guest; the cache-reclaim loop survives its exec; `exec` returns guest exit codes; guest frpc reaches host frps through the gateway and Project URLs serve locally and remotely; a DSH prompt reaches the host model proxy through `100.96.0.1:8317`; DSH sessions return from the Sync Store; reported host memory falls after cache reclaim; a three-mount DevVM boots; the VM process does not stay at high CPU after connections close.
