# Spec: DevVM refactor onto Smolvm 1.27.3 with native Windows support

Labels: ready-for-agent

Source plan with code references, evidence, and review history: [plan.md](plan.md).

## Problem Statement

A DevVM user runs one isolated microVM per Project, managed from the Control Daemon's web app, with DSH Runtimes, Session Sync, Project URLs, and tailnet access. Today:

- DevVMs run on Smolvm 1.19.3. Newer releases fix bugs that affect this workload (more mounts with virtio-net, exec processes killed when the workload exits, Windows support), and the current Linux root install path changes in newer releases.
- The user cannot run DevVM natively on a Windows workstation. The `devvm` command is a bash script, setup is a bash script, the service manager supports only systemd and launchd, and command cancellation relies on Unix process groups.
- Each workstation must build a 5 GB guest image with Docker and import it as a tar, which is slow and requires Docker on every host.
- Two web interfaces exist: the Project app and an older management page embedded in it, with two lifecycle APIs that behave differently on cancellation. Registering a Project, opening a guest port, or configuring Session Sync still requires the older page.
- Configuration is spread over a Smolfile, several JSON files, command-line flags, and a dozen environment variables. One consequence: `sync setup` writes its configuration to a path the Control Daemon does not read on macOS and Windows.
- Inside the code, the Control Daemon drives DevVMs by running the separate `devvm` script and parsing its text output, so every DevVM operation crosses two processes and a text contract.
- DevVM lives as a subdirectory of a personal configuration repository with no automated checks or releases.

## Solution

DevVM becomes a single `devvm` program in its own GitHub repository:

- One binary runs the Control Daemon (`devvm serve`), the Project commands (`devvm start`, `shell`, `exec`, `stop`, `status`, `memory`, `rm`, `name`), installation (`devvm setup`), service management, and Session Sync setup, on Linux, macOS, and Windows x86_64.
- DevVMs run on Smolvm 1.27.3, which `devvm setup` downloads, verifies, and installs into DevVM's own data directory.
- The guest image is built by CI for every release and published to GitHub Container Registry; a new DevVM pulls it on its first start.
- The Project app is the only web interface and the only lifecycle API is daemon-owned operations.
- One `config.toml` holds every setting.
- GitHub Actions checks every push and publishes release binaries and images from version tags.
- Existing installations move over through one clean reinstall: delete the old DevVMs, install the new `devvm`, recreate the DevVMs; Portable DSH State returns from the Sync Store.

The guest environment, DSH Runtime behavior, Session Sync, Project Logs, Project URLs with the Loopback Facade, and tailnet access stay as they are on every OS.

## User Stories

Installation and upgrade

1. As a DevVM user, I want to install DevVM by cloning its repository and downloading one release binary, so that I do not need a Rust toolchain or Docker on my workstation.
2. As a DevVM user, I want `devvm setup` to install the pinned Smolvm version for me, so that every workstation runs the same tested Smolvm.
3. As a DevVM user, I want setup to verify the downloaded Smolvm and frps archives against their published checksums, so that a corrupted or tampered download is rejected.
4. As a DevVM user, I want Smolvm installed inside DevVM's own data directory, so that it does not conflict with any other Smolvm installation and its version is unambiguous.
5. As a DevVM user, I want setup to install the host frps at the same version as the guest's frpc, so that Project ingress keeps working.
6. As a DevVM user, I want setup to tell me exactly which prerequisite is missing (KVM access, Hypervisor Platform, Developer Mode, Git symlinks, OpenSSH), so that I can fix my host without guessing.
7. As a DevVM user, I want setup to record where my shared guest configuration (`root/`) lives and fail if its DSH profiles are missing, so that a DevVM never starts without its profiles.
8. As a DevVM user, I want rerunning setup with new options (for example adding a remote domain) to update only those settings, so that I can enable remote access later without losing my configuration.
9. As a DevVM user, I want `devvm setup --service` to register and start the Control Daemon as a user service, so that it runs after every login.
10. As a DevVM user, I want setup to print the host Caddy site block and its environment settings when I configure a remote domain, so that I can set up remote HTTPS access.
11. As a DevVM user, I want a documented clean reinstall procedure, so that I can move from the old installation to the new one without losing DSH sessions.
12. As a DevVM user, I want the reinstall procedure to tell me to confirm my Sync Status is synchronized before deleting DevVMs, so that unsynced sessions are not lost.
13. As a DevVM user without a Sync Store, I want to know to copy my VM-local DSH state out of the guest before deleting a DevVM, so that I keep my sessions.
14. As a DevVM user, I want the old installation to remain usable until its cleanup is finished, so that I can delete old DevVMs with the old tools.

DevVM lifecycle

15. As a DevVM user, I want `devvm start` in a Project directory to create and start its DevVM, so that I can begin working with one command.
16. As a DevVM user, I want a new DevVM to pull the published guest image on its first start, so that I never build the image myself.
17. As a DevVM user, I want later starts of a DevVM not to pull the image again, so that restarts are fast and work offline.
18. As a DevVM user, I want the DevVM's CPU count, memory, and storage size to come from my configuration, so that I can size DevVMs for my workstation.
19. As a DevVM user, I want the Project directory mounted at `/root/workspace` and the shared configuration at `/devvm-root`, as today, so that my tools and agents see the same paths on every workstation.
20. As a DevVM user, I want guest setup (ingress, shared configuration links, storage cleanup, the file-cache reclaim loop) to run only when a DevVM actually starts, so that repeated `start` calls are harmless.
21. As a DevVM user, I want a DevVM start to fail with a clear error when DSH or a DSH profile is missing in the guest, so that I learn about a broken setup instead of getting a silently degraded DevVM.
22. As a DevVM user, I want `devvm shell` (also the default command) to open an interactive guest shell, so that I can work inside the DevVM.
23. As a DevVM user, I want `devvm exec -- <command>` to stream output and return the guest command's exit code without starting the DevVM, so that I can script guest commands.
24. As a DevVM user, I want `devvm status` to show whether the Project's DevVM exists and its state, so that I can check it from a terminal.
25. As a DevVM user, I want `devvm memory` to print host, guest, and configured memory as JSON, so that I can see how much memory the DevVM really uses.
26. As a DevVM user, I want `devvm stop`, `devvm rm`, and `devvm name` to keep working, so that my terminal workflow is unchanged.
27. As a DevVM user, I want freed guest file cache returned to the host on every OS, so that idle DevVMs do not hold host memory.
28. As a DevVM user, I want a DevVM with three or more host mounts and virtio-net to boot, so that I am not limited by the old interrupt budget.
29. As a DevVM user, I want two Projects whose directory names differ only in repeated hyphens to get Project URLs that actually route, so that the Control Daemon's links and the guest's ingress registration always agree.

Control Daemon and Project app

30. As a DevVM user, I want the Project app to be the only DevVM web page, so that every task happens in one place on desktop and phone.
31. As a DevVM user, I want to register a Project in the Project app by browsing directories beneath my home, so that I no longer need the management page.
32. As a DevVM user, I want to unregister a Project in the Project app, so that I can remove it from the list without deleting its DevVM or Sync Store data.
33. As a DevVM user, I want to see each Project's memory use, Project ID, and Project host in the Project app, so that I have the details the management page showed.
34. As a DevVM user, I want separate local and remote DSH links for a Project, so that I can open DSH from the workstation or over the tailnet.
35. As a DevVM user, I want to open a guest port from the Project app and get its Project URL, so that I can reach development servers in the DevVM.
36. As a DevVM user, I want to view and run Session Sync setup from the Project app, so that I can configure the Sync Store without the command line.
37. As a DevVM user, I want to delete a Project's Sync Store data from the Project app, so that I can clean up a Project's remote state.
38. As a DevVM user, I want log source filters, an errors-only toggle, and Follow in the Project app's Project Log viewer, so that I can diagnose problems as easily as before.
39. As a DevVM user, I want every lifecycle action (start/stop/delete VM, launch/stop/restart DSH) to run as a daemon-owned operation that continues after I close the page, so that a dropped phone connection does not interrupt it.
40. As a DevVM user, I want a conflicting start to be rejected while another operation runs, and stop or delete to cancel an active start and wait for its cleanup, so that two clients cannot corrupt a Project's state.
41. As a DevVM user, I want every client to see the same active and latest operation for a Project, so that my phone and desktop agree.
42. As a DevVM user, I want the Control Daemon's address, ports, and remote domain to come from the configuration file, so that the service definition never needs editing.
43. As a DevVM user, I want `devvm sync setup` to take effect in the running Control Daemon immediately, so that I do not need to restart it after configuring Session Sync.
44. As a DevVM user, I want Session Sync provisioning to keep my workstation's writer identity across re-provisioning, so that Sync Store ordering stays correct.
45. As a DevVM user, I want DSH Runtime status, Sync Status, and Project Logs to behave exactly as today, so that the refactor does not change my daily workflow.

Native Windows

46. As a Windows user, I want to run DevVM natively without WSL2, so that DevVMs run directly on the Windows Hypervisor Platform.
47. As a Windows user, I want Projects under `C:\Users\<me>\...` mounted at `/root/workspace`, so that Windows Projects behave like Linux and macOS Projects.
48. As a Windows user, I want the Control Daemon to start at logon as a per-user Scheduled Task without admin rights, so that it has access to my DevVMs and runs indefinitely.
49. As a Windows user, I want `devvm service status` and `uninstall` to manage that task, so that service management works as on other OSes.
50. As a Windows user, I want lifecycle commands started by the Control Daemon to finish and report their result, so that a start never hangs waiting for output held open by the running VM.
51. As a Windows user, I want cancelling a start to stop the Smolvm command and its helper processes while leaving a started VM and frps running, so that cancellation behaves as on Linux and macOS.
52. As a Windows user, I want `devvm shell` to refuse with a clear message when it is not run from a console, so that it never hangs.
53. As a Windows user, I want freed guest memory returned to Windows promptly, so that DevVMs do not keep memory Windows needs.
54. As a Windows user, I want Session Sync setup to check for OpenSSH and protect my private key's permissions, so that Windows OpenSSH accepts the key.
55. As a Windows user, I want remote access through Tailscale and a host Caddy Windows service, documented step by step, so that my Windows workstation serves Project URLs over the tailnet.
56. As a Windows user, I want the shared skills symlinks in `root/` to resolve inside the guest, so that agents see the same skills as on other OSes.

Repository, CI, and releases

57. As the maintainer, I want DevVM in its own private GitHub repository with its history, so that it is developed and released independently of my personal configuration repository.
58. As the maintainer, I want every push and pull request checked for formatting, lints, and tests on Linux, macOS, and Windows, so that platform breakage is caught before release.
59. As the maintainer, I want pushing a version tag to build release binaries for every supported OS and publish them with checksums, so that releasing is one command.
60. As the maintainer, I want the release to fail when the tag does not match the crate version, so that binary and image versions always agree.
61. As the maintainer, I want the guest image built natively for amd64 and arm64 and published under one multi-architecture tag, so that every host pulls the right image.
62. As the maintainer, I want the image package to be public while the repository stays private, so that workstations pull the image without credentials.
63. As the maintainer, I want to develop the guest image locally by pointing my configuration at a locally built image, so that I can test image changes before releasing.
64. As the maintainer, I want an explanation of each GitHub Actions concept in GitLab CI terms, so that I can maintain the workflows with my GitLab experience.

Agents working on the code

65. As an agent, I want every external command the program runs to go through one command boundary that tests can mock, so that tests assert exactly which Smolvm, SSH, download, and Task Scheduler commands are issued.
66. As an agent, I want configuration passed explicitly instead of read from environment variables, so that tests run in parallel without interfering.
67. As an agent, I want `AGENTS.md` and the readme updated to the new commands, configuration, and test rules, so that future work follows the new architecture.

## Implementation Decisions

Program structure

- One binary, `devvm`. Modules: configuration; Project identity and registry; the Smolvm boundary (argument builders and machine-list parsing); DevVM lifecycle; host ingress; DSH Runtime; operation coordination; Session Sync; Project Logs; Project Browser; HTTP API and Project app; service registration; setup; command runner.
- The Control Daemon and the Project commands call the DevVM lifecycle module in-process. The separate `devvm` script, its text output contract, and the daemon's parsers for it are removed.
- Every module receives the loaded configuration; no module reads environment variables. The variables passed into the guest (the Project host for frpc, and the DSH Runtime's embed Project ID, Control origins, and Control domain) remain.

Command line

- `devvm setup [--root-dir] [--remote-domain] [--remote-ip] [--service]`: check prerequisites, install Smolvm and frps, create `config.toml` (root directory required) or update only the given keys, print the embedded host Caddy block when a remote domain is configured, and with `--service` register, enable, and start the service.
- `devvm serve`: run the Control Daemon; no options.
- `devvm service install|status|uninstall`: the registered command is the absolute `devvm` path with `serve`, plus `--config` when a non-default configuration file is used. Install always registers, enables, and starts.
- `devvm sync setup --ssh-user --ssh-host --ssh-key [--ssh-port] [--remote-root] [--no-verify]`: the existing flags; the key stays required. One provisioning function serves the command and the HTTP API.
- `devvm start|shell|exec|stop|status|memory|rm|name`: Project commands in a Project directory; `shell` is the default; `exec` accepts the command with or without `--`, passes stdio through, returns the guest exit code, and never creates or starts the DevVM; `memory` prints `host_bytes`, `guest_bytes`, `limit_bytes`, with null for unavailable values.
- A global `--config` selects the configuration file.

Configuration and state

- `config.toml` in the OS configuration directory, with: `root_dir` (required); `[server]` `host` (default loopback), `port` (8100), `ingress_port` (8102); `[vm]` `cpus`, `memory_mib`, `storage_gib`, optional `image`; `[remote]` `domain`; `[sync]` `ssh_user`, `ssh_host`, `ssh_port`, `ssh_key`, `remote_root`. Unknown keys and missing required values are errors. Programmatic updates preserve the rest of the file and its comments.
- The data directory holds the Project registry, the installed Smolvm, frps and its log, and on Windows the command-capture directory.
- Project Logs and the DSH launch token stay under the shared root directory, where the guest writes them.
- Session Sync provisioning writes `[sync]` and the guest-visible sync file with SSH user, host, port, the guest key path (the key copied into the shared root's SSH directory), remote root, the existing writer ID kept or a new one minted, and the Control Daemon URL. The daemon reads sync settings from the configuration file when needed.
- Removed: the Smolfile and its example, host-side `DEVVM_*` settings, and the path flags (registry path, log directory, home directory, `devvm` binary path, service binary path, sync config path). The frps bind port 17007 is a constant because the guest's frpc names it.

Smolvm

- Version 1.27.3, pinned in code; pin a newer release instead if it fixes the virtio-net TCP relay CPU spin and passes acceptance.
- Installed by downloading the platform archive and the release checksum file with the system `curl`, verifying SHA-256, and extracting with the system `tar` to a versioned directory in the data directory; run by absolute path.
- DevVMs are created with explicit flags: CPUs, memory, storage, network with the virtio-net backend, working directory `/root`, the guest environment variable, and the two mounts.
- Machine state comes from `machine ls --json`, once per status refresh for all Projects: `running` is Running; `created` and `stopped` are Stopped; any other state is Failed. `machine status` is not used because it reports a missing machine and a real failure with the same exit code.
- Memory: host bytes from the VM process ID in the machine list, guest bytes from the guest's `/proc/meminfo`, the limit from the configured memory.

Identity

- One identity rule for the Control Daemon and the Project commands: the daemon's existing Project host rule (sanitized directory name and an eight-character path hash; hash only when the name is over 48 characters). The machine name is `dev-<name>-<hash>`.

Guest setup

- Guest scripts are kept as files embedded into the binary. The shared configuration linking keeps its bind mounts and profile lockfile check but drops the migration of pre-bind-mount layouts and the skips for missing DSH or profiles.

frps

- Version 0.71.0 on the host and in the guest; ingress arguments unchanged. A running frps owned by the current user with matching executable and arguments is reused; otherwise it starts detached with output appended to its log.

Guest image

- Published per release as `ghcr.io/risakirisu/devvm:<version>` and `:latest` for amd64 and arm64; public package. The default image reference is derived from the binary's version. The configuration's `image` overrides it with another reference or a local `docker save` tar.

Web interface and lifecycle API

- The Project app gains Project registration with the Project Browser, unregister, memory, Project ID and host, separate local and remote DSH links, open port, Session Sync setup and Sync Store deletion, and log source filters, errors-only, and Follow.
- The management page, its embedding dialog, the blocking lifecycle routes, and the request-bound operation path with disconnect cancellation are removed. Daemon-owned operations are the only lifecycle API; their coordination rules (conflict rejection, stop/delete cancelling an active start and waiting for cleanup) are unchanged.

Process handling

- Unix: cancellation keeps sending SIGTERM to the command's process group and waits for exit and output closure.
- Windows: `devvm` clears inheritance on its own standard handles at startup, and the command runner captures output through files read after exit, because Windows process creation passes every inheritable handle to a child and a detached VM or frps process would otherwise hold a caller's output open. Cancellation snapshots the spawned command's descendants while it runs, terminates the command and its descendants except Smolvm's VM processes (with their subtrees) and processes whose command line cannot be read, and waits only for the terminated processes.

Windows specifics

- Prerequisites: Windows 10 1809 or later on x86_64, the Hypervisor Platform feature, Developer Mode, Git with symlinks enabled, and OpenSSH when Session Sync is configured.
- Every Smolvm command gets `SMOLVM_WHP_RECLAIM=discard`, so freed guest pages are discarded continuously instead of only under host memory pressure or after a reclaim pulse.
- `devvm shell` requires a console and exits 1 with a message otherwise.
- frps starts detached in its own process group with its log file as output.
- The service is a Scheduled Task registered from XML with a logon trigger and principal for the current user's SID, interactive token, least privilege, and no execution time limit.
- Session Sync provisioning refuses a private key readable by other users (with an `icacls` instruction) and restricts the copied key and shared SSH directory to the current user.

Repository and CI

- New private repository `RisaKirisu/devvm`, extracted from the personal configuration repository's DevVM subdirectory with its history. The old subdirectory stays until the clean reinstall has used it, then is removed.
- Line endings are forced to LF for the Dockerfile, scripts, patches, the shared root, shell scripts, and TOML files.
- `ci` workflow on pushes and pull requests: format check, lints as errors, and tests on Linux; tests on macOS and Windows once each compiles there.
- `release` workflow on `v*` tags: release binaries per OS on native runners (Windows added with native Windows support), the image per architecture on native Linux runners pushed by digest, a manifest job combining the digests under the version and `latest`, and a release job publishing the binaries with a checksum file. Image jobs have read access to contents and write access to packages; the release job has write access to contents.

Delivery order

- Repository and CI first; then the single binary, Smolvm 1.27.3, and one configuration on Linux and macOS, followed by the clean reinstall; then release automation; then the single web interface; then native Windows. Each step ships on its own.

## Testing Decisions

- A good test drives behavior through the highest interface that exhibits it and asserts externally visible results: HTTP responses, Project command results, files written, and the external commands issued. It does not assert internal structure.
- One mock seam: the command runner. Tests use the mockall runner to assert each external command in order (program, arguments, guest script) and return the tool's documented output, such as the machine list JSON. No fake executables on `PATH`, no stubbed tools, no hand-written mocks.
- Control Daemon behavior is tested through its HTTP API with the mocked runner, following the existing API tests: status from the machine list, lifecycle operations and their Smolvm and guest command sequences, Session Sync provisioning and immediate readback, Project registration, and Project Logs.
- Project commands, setup, Session Sync setup, and service management are tested through their command functions with the mocked runner: create/start sequences, guest setup only on an actual start, `exec` exit codes, `memory` with missing values, setup's download, verify, and extract commands, configuration creation and key updates, and service registration content per platform without registering a real service.
- Plain functions with plain tests: identity (including repeated hyphens and names over 48 characters), machine-state mapping, configuration validation, checksum verification on real files, archive names.
- Configuration is isolated per test with a temporary configuration file; no environment variables.
- Real processes where process handling is the behavior: cancellation of real child processes on Unix (existing lifecycle tests) and Windows (spawned command and a running descendant both gone, preserved processes untouched), and a failed command spawn reported to tracing and the Project Log (existing observability test, retargeted at a non-executable Smolvm).
- Operation coordination: conflicting submissions rejected; stop and delete cancel an active start and wait for cleanup; two clients see the same active and latest operation. These replace the request-disconnect tests.
- The Project app's new panels are checked with the existing Playwright scripts for controls and mobile layout against the fixture daemon.
- The opt-in live acceptance test runs real Smolvm, DSH, Caddy, and frps on Linux and macOS; Windows acceptance runs on the maintainer's Windows machine.
- Unix-only tests (file modes, process groups, the Linux browser fixture, the bash-based DSH probe test) are gated to Unix; the shared test helpers build exit statuses for both platforms.
- Tests that locate tools use a cross-platform lookup; the live test's unused host rsync requirement is removed.

## Out of Scope

- Windows arm64 (Smolvm ships no Windows arm64 build) and WSL2.
- Changes to the guest image's contents, DSH, the DSH patches, the Session Sync plugin, and the ingress chain (host Caddy, frps, guest frpc, guest Caddy).
- Running the DSH plugin test suites in CI; they need a staged DSH environment that is not scripted today and keep running inside a DevVM.
- Migrating existing DevVMs, machine names, or configuration; the clean reinstall replaces them.
- Creating the GitHub repository, making the image package public, the clean reinstall, host Caddy installation, and Windows acceptance runs: the maintainer performs these.

## Further Notes

- Known Smolvm 1.27.3 risks: the virtio-net TCP relay can spin at 100% CPU after a half-closed then reset connection (an unmerged fix exists upstream); on Linux, a new DevVM's default-size overlay disk can be qcow2 and does not shrink after trim.
- A new DevVM's first start pulls the image into its own storage, so it takes longer and needs network access.
- The image build must fit the hosted runner's advertised 14 GB disk; the first release run measures it and frees preinstalled toolchains if needed.
- Windows virtiofs over NTFS differs from ext4 in file modes, case handling, and hard links; Windows acceptance runs git, pnpm, and cargo in a Project.
- The 19-interrupt x86_64 device budget has not been measured on the Windows Hypervisor Platform; Windows acceptance includes a three-mount DevVM.
- A private repository on GitHub's Free plan has 2,000 Actions minutes per month; macOS runners cost about ten times the Linux per-minute price. GitHub Container Registry storage and downloads are free.
