# DevVM Workspace Supervision

Status: resolved

## Problem Statement

The user has a lightweight shell-based system that creates one hardware-isolated DevVM per Project with SmolVM and exposes guest HTTP ports through FRP and Caddy. Using it still requires opening a project terminal, entering the VM, launching DSH, and keeping that shell open. There is no central view of Projects, DevVM status, DSH Runtime status, or collected logs.

Applications reached through routed Project URLs see external authority information. DSH and Better Sidebar apply localhost-related checks that can return HTTP 403. The user does not want every application or plugin to require its own trusted-host configuration.

DSH history is currently mixed into workstation-wide state. The user needs Project-specific DSH history to follow the Project between Linux and macOS workstations without committing chat logs to Git, depending on a clean shutdown, using a filesystem watcher, or replacing DSH persistence with Postgres.

## Solution

Build one complete version-one system around the existing DevVM CLI.

A Rust Control Daemon with one embedded vanilla HTML/JavaScript page will register Projects, browse directories beneath the daemon user's home, create stable Project IDs, show and control DevVMs, launch and monitor DSH Runtimes, generate Project URLs for DSH and arbitrary guest ports, display Sync Status, and expose Project Logs. It can run from the command line or through an optional Linux or macOS user service.

Ingress will apply a Loopback Facade to every proxied guest port. Local Project URLs will continue using localhost subdomains. Remote Project URLs will use a private wildcard domain over Tailscale. A separate DNS mode in the same Rust binary will serve the workstation wildcard mapping after one-time privileged setup. Tailscale membership is the only remote access boundary.

A shared DSH plugin will synchronize Portable DSH State to a VPS Sync Store through rsync over SSH. It will run Session Sync after DSH saves a completed turn, workspace change, or message-feedback change, and when the user requests it manually. It will retry simple failures and expose a clickable status indicator in DSH and the central UI. DSH will retain its local file persistence inside the DevVM.

## User Stories

1. As a developer, I want one complete version-one system, so that local control, synchronization, and tailnet access work together.
2. As a developer, I want to run the Control Daemon from the command line, so that a host service is optional.
3. As a Linux developer, I want optional systemd user integration, so that the Control Daemon can start automatically.
4. As a macOS developer, I want optional launchd user integration, so that the Control Daemon can start automatically.
5. As a developer, I want one setup entry point, so that the complete system can be configured coherently.
6. As a developer, I want the existing DevVM CLI to remain usable, so that shell workflows do not depend on the Web UI.
7. As a developer, I want the Control Daemon to reuse existing DevVM behavior, so that VM operations do not have competing implementations.
8. As a developer, I want to open the central UI locally, so that I can manage Projects without separate project terminals.
9. As a developer, I want to open the central UI from a tailnet device, so that I can manage Projects remotely.
10. As a developer, I want Tailscale membership to be the only remote access boundary, so that I do not maintain another login.
11. As a developer, I want to see registered Projects and their statuses, so that I can choose what to open.
12. As a developer, I want to browse directories beneath my home directory, so that I can register a Project without typing its full path.
13. As a developer, I want the Project Browser limited to my home directory, so that it does not expose the rest of the host filesystem.
14. As a developer, I want registration to create a Project ID when one is absent, so that a Project receives portable identity automatically.
15. As a developer, I want registration to reuse an existing Project ID, so that another clone of the Project reaches the same Sync Store directory.
16. As a developer, I want the Project ID stored in `.devvm-id`, so that it can follow the Project through Git without storing credentials.
17. As a developer, I want every Project mounted at `/root/workspace` inside its DevVM, so that DSH records a stable working directory.
18. As a developer, I want the UI to show DevVM status, so that I know whether the Project environment is running.
19. As a developer, I want to start and stop a DevVM from the UI, so that I do not need a project terminal.
20. As a developer, I want local DevVM deletion to be explicit, so that it is not confused with unregistering the Project or deleting synchronized data.
21. As a developer, I want to unregister a Project without deleting its DevVM or Sync Store data, so that dashboard organization is non-destructive.
22. As a developer, I want Sync Store deletion to be a separate confirmed action, so that remote DSH history is not deleted accidentally.
23. As a developer, I want DevVM and DSH Runtime lifecycles to remain separate, so that I can use a VM without running DSH.
24. As a developer, I want a Launch DSH action, so that the daemon starts the DevVM and DSH Runtime when needed.
25. As a developer, I want Launch DSH to tolerate an already-running DSH Runtime, so that repeated clicks do not create duplicates.
26. As a developer, I want independent DSH Runtime status, so that I can distinguish VM status from DSH status.
27. As a developer, I want a link when DSH is running, so that I can open it in a separate browser tab or window.
28. As a developer, I want unexpected DSH exit shown as failed, so that I can inspect the failure.
29. As a developer, I want DSH restart to be manual, so that the daemon does not hide failures through automatic restart.
30. As a developer, I want to use Better Sidebar's terminal, so that the central UI does not need another terminal.
31. As a developer, I want an Open Port control, so that I can enter a guest port and open its Project URL.
32. As a developer, I want Open Port to generate a link without managing the guest process, so that generic application support stays small.
33. As a developer, I want local Project URLs to keep the existing localhost-subdomain form, so that local use requires no DNS setup.
34. As a developer, I want remote Project URLs to use wildcard private names over Tailscale, so that arbitrary guest ports retain host-based routing.
35. As a developer, I want wildcard DNS served by the workstation, so that the VPS remains dedicated to DSH synchronization.
36. As a developer, I want the DNS process separated from the Control Daemon while remaining in the same Rust binary, so that only DNS mode receives port-53 privilege.
37. As a developer, I want one-time privileged DNS setup, so that normal daemon and CLI operation remain unprivileged.
38. As a developer, I want every proxied guest port to receive the Loopback Facade, so that server-side localhost checks do not require per-application configuration.
39. As a developer, I want application-facing Host and same-origin Origin values presented as loopback values, so that DSH and Better Sidebar server checks work through Project URLs.
40. As a developer, I want DSH browser-side localhost recognition to support local and tailnet Project URLs, so that browser-only checks also work.
41. As a developer, I want Portable DSH State to remain on the DevVM filesystem, so that DSH keeps its existing local persistence behavior on Linux and macOS hosts.
42. As a developer, I want workstation-wide DSH credentials, plugins, profiles, skills, presets, and settings to remain centrally shared, so that they are managed once per workstation.
43. As a developer, I want session logs synchronized, so that chat history follows the Project.
44. As a developer, I want authoritative attachment objects synchronized, so that historical image messages follow the Project.
45. As a developer, I want workspace registry data synchronized, so that workspace titles, ordering, archive state, and session grouping follow the Project.
46. As a developer, I want message-feedback data synchronized when present, so that ratings and notes follow the Project.
47. As a developer, I want projection cache data excluded, so that rebuildable cache state is not synchronized.
48. As a developer, I want synchronization to be optional, so that local DevVM and DSH use works without a VPS.
49. As a developer, I want a one-time VPS setup wizard, so that the shared credential, remote directory, and rsync access can be configured and verified.
50. As a developer, I want all Projects to share one sync credential in version one, so that credential management remains simple.
51. As a developer, I want Sync Store directories grouped by Project ID, so that synchronization does not depend on host paths.
52. As a developer, I want rsync to run inside the DevVM, so that it can directly read VM-local Portable DSH State.
53. As a developer, I want Session Sync to run after a completed DSH turn, so that completed conversation work reaches the Sync Store promptly.
54. As a developer, I want Session Sync to wait until DSH finishes saving the completed turn before copying it.
55. As a developer, I want workspace and message-feedback changes to start Session Sync, so that changes outside conversation turns are included.
56. As a developer, I want projection-cache changes ignored by Session Sync, so that cache updates do not cause transfers.
57. As a developer, I want to run Session Sync manually, so that I can retry without sending another DSH message.
58. As a developer, I want only one synchronization operation running at once, so that transfers do not overlap.
59. As a developer, I want triggers during an active transfer folded into one follow-up transfer, so that changes are included without starting parallel rsync processes.
60. As a developer, I want a failed transfer retried after one second for at most five total attempts, so that brief failures recover simply.
61. As a developer, I want retries to stop after the fifth failed attempt, so that a persistent failure does not create an endless loop.
62. As a developer, I want the next completed turn, relevant saved change, or manual Session Sync to retry Dirty Local State, so that a later successful sync recovers it.
63. As a developer, I want local state marked dirty before background synchronization, so that an interrupted transfer is recognized at startup.
64. As a developer, I want clean or fresh local state pulled from the Sync Store before DSH starts, so that another workstation receives synchronized history.
65. As a developer, I want Dirty Local State pushed before pulling, so that unsynchronized local work is not overwritten.
66. As a developer, I want existing local state to open with Degraded Sync when the VPS is unavailable, so that temporary network loss does not block work.
67. As a developer, I want DSH startup blocked when the Sync Store is configured but unreachable and no local Portable DSH State exists, so that a fresh workstation does not create divergent empty history.
68. As a developer, I want version one to rely on the Single Writer Rule without conflict detection or a distributed lease, so that synchronization remains simple.
69. As a developer, I want Sync Status shown in both the central UI and DSH, so that synchronization failure is visible wherever I am working.
70. As a developer, I want the DSH indicator green when synchronized, animated while synchronizing, and yellow after failure, so that state is recognizable at a glance.
71. As a developer, I want to click the DSH indicator to start a manual synchronization, so that retry is discoverable.
72. As a developer, I want DSH and plugin output collected, so that DSH startup and synchronization failures are traceable.
73. As a developer, I want Control Daemon operations collected, so that VM commands and lifecycle failures are traceable.
74. As a developer, I want Project Logs stored on the host, so that they remain available after a DevVM is stopped or deleted.
75. As a developer, I want recent Project Logs visible in the central UI, so that I can diagnose failures without a shell.
76. As a developer, I want Linux and macOS supported in version one, so that I can use the same system across my workstations.
77. As a developer, I want no automatic migration of current shared DSH state or existing VMs, so that the current setup remains untouched.
78. As a developer, I want no generic execute endpoint in the central HTTP interface, so that the UI exposes only the lifecycle actions we agreed on.
79. As a developer, I want the complete system kept off the public Internet, so that it remains a personal local-and-tailnet development environment.

## Implementation Decisions

- Build one Rust binary containing the Control Daemon, embedded vanilla HTML/CSS/JavaScript UI, foreground serve mode, and a separate wildcard DNS mode. Do not add a frontend framework or database.
- Preserve the existing DevVM CLI as the owner of VM operations and keep its current shell, start, stop, execute, status, and removal workflows available without the daemon.
- Keep the Control Daemon's registry as a small local file containing registered host paths and Project IDs.
- Limit the Project Browser to the daemon user's home directory.
- Create and reuse UUID Project IDs through `.devvm-id`.
- Mount each Project at `/root/workspace` and launch DSH from that working directory.
- Expose only the agreed project registration, browsing, lifecycle, link, log, synchronization, unregister, local deletion, and remote deletion operations. Do not expose generic command execution.
- Bind the management surface for local and Tailscale use, with Tailscale membership as the only remote authentication layer.
- Track DevVM and DSH Runtime status separately. Launch DSH explicitly and idempotently; report unexpected exit and require manual restart.
- Generate local and tailnet links for DSH and arbitrary guest ports without managing arbitrary guest processes.
- Preserve FRP virtual-host routing and Caddy guest-port dispatch. After selecting the guest port, present upstream Host and matching same-origin Origin as loopback authority for every proxied port.
- Extend the existing DSH browser compatibility patch to the local and tailnet Project URL suffixes.
- Implement wildcard private DNS as a separate mode of the Rust binary and configure Tailscale split DNS to point the private suffix at the workstation.
- Keep foreground daemon operation available on Linux and macOS, with optional systemd user and launchd user integration.
- Keep Portable DSH State on the DevVM filesystem. Continue sharing workstation-wide DSH configuration centrally.
- Synchronize only session logs, authoritative attachment objects, workspace registry data, and message-feedback data. Exclude projection caches and workstation-wide DSH configuration.
- Make synchronization optional. Group Sync Store data by Project ID and use one shared SSH/rsync credential.
- Run rsync inside the DevVM from a shared DSH host/client plugin.
- Run Session Sync after DSH saves a completed turn, workspace change, or message-feedback change. Ignore projection-cache changes and provide manual synchronization.
- Use one synchronization operation at a time. Fold triggers arriving during synchronization into one later pass.
- Retry after one second for five total attempts. After final failure, retain Dirty Local State and wait for the next completed turn, relevant saved change, or manual retry.
- Show Sync Status in the central UI and through a clickable DSH overlay indicator using the agreed green, animated, and yellow states.
- On DSH startup, pull when local state is clean or absent and push Dirty Local State before pulling. Permit Degraded Sync with existing local state. Block a fresh local start when configured remote state cannot be reached.
- Rely on the Single Writer Rule without leases, conflict detection, or merge behavior.
- Provide a one-time VPS setup wizard for the shared credential, Project-ID directory root, and rsync verification.
- Capture DSH/plugin, ingress, and Control Daemon output into host-persisted per-Project logs and expose recent logs in the central UI.
- Keep unregister, local DevVM deletion, and Sync Store deletion separate. Require confirmation for Sync Store deletion.
- Support Linux and macOS. Leave current shared DSH data and existing VMs untouched.

## Testing Decisions

- Tests verify external behavior rather than private implementation details.
- The primary seam is the real Control Daemon HTTP interface running against temporary state and a fake DevVM CLI. It covers the agreed Project Browser, Project registration, lifecycle, link generation, Project Logs, Sync Status, and separated removal behavior.
- The DSH plugin seam composes real Cordis, SessionStore, JSONL persistence, and storage-domain services in a temporary DSH home. It verifies real completed-turn and saved-domain events, save ordering, exact Session Sync categories with real local rsync, profile isolation, and Web route activation without hand-built mocks.
- The ingress seam runs real Caddy against a header-echo application and verifies Project URL routing plus the agreed Loopback Facade behavior. FRP receives a focused configuration/smoke test rather than tests of FRP internals.
- The DNS seam runs the real DNS mode and verifies wildcard private-name resolution to the workstation address.
- An opt-in system acceptance seam runs the real SmolVM, DSH, FRP, Caddy, rsync/SSH, DNS, and Tailscale flow on Linux and macOS.

## Out of Scope

- Replacing DSH file persistence with a remote database.
- Filesystem watchers or shutdown-dependent synchronization.
- Concurrent Project writers, distributed leases, conflict detection, or merge behavior.
- Per-project sync credentials.
- Additional application login or public Internet exposure.
- Automatic migration of existing shared DSH state or existing VMs.
- A standalone terminal in the central UI.
- Generic command execution from the central HTTP interface.
- Named application process management beyond DSH and arbitrary-port link generation.
- Automatic DSH restart.
- Project browsing outside the daemon user's home directory.
- A transparent or per-application ingress mode in version one.

## Further Notes

- DSH already writes the working directory into session metadata. The system standardizes the guest working directory; it does not add another cwd field.
- FRP and Caddy cannot change the hostname visible to browser JavaScript. The Loopback Facade handles server-side checks; DSH's browser-side compatibility patch remains separate.
- Remote devices need wildcard private DNS because localhost names resolve to the browsing device. The VPS remains dedicated to Portable DSH State synchronization.
- The Sync Store is a Project-ID directory tree containing only the agreed Portable DSH State categories.
- Version one trusts the user to follow the Single Writer Rule.
