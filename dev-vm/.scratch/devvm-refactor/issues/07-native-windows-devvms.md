# 07: Native Windows DevVMs

**What to build:** A Windows user runs DevVM natively on the Windows Hypervisor Platform with the same behavior as on Linux and macOS: DevVMs for Projects under the user's home, a Control Daemon started at logon, Session Sync, and remote access over Tailscale.

**Blocked by:** 04, 06

**Status:** ready-for-agent

- [ ] Projects under `C:\Users\...` mount at `/root/workspace`; every Smolvm command runs with `SMOLVM_WHP_RECLAIM=discard`.
- [ ] frps starts detached in its own process group with its log file as output.
- [ ] `devvm shell` exits 1 with `devvm shell requires a console` when stdin is not a console.
- [ ] `setup` installs the Windows Smolvm and frps archives and checks Windows 10 1809+ x86_64, the Hypervisor Platform feature (printing the command to enable it), Developer Mode, Git symlinks, and OpenSSH when Session Sync is configured.
- [ ] `sync setup` refuses a private key readable by other users with an `icacls` instruction and restricts the copied key and shared SSH directory to the current user.
- [ ] `service install` registers a per-user Scheduled Task from XML (logon trigger and principal for the current user's SID, interactive token, least privilege, no execution time limit) running `devvm serve`; `status` and `uninstall` manage it; tests assert the XML and never register a real task.
- [ ] The release workflow adds the Windows binary.
- [ ] The readme documents Windows prerequisites and setup; the remote-access doc documents host Caddy as a Windows service and Tailscale.
- [ ] Maintainer, on Windows 11: ticket 03's guest, exec, ingress, model proxy, memory, three-mount, and remote-access checks pass; `/root/.dsh/skills` resolves in the guest; captured starts return with frps running and not running; cancelling a start (including a first start) releases the Project and a later start succeeds; the Scheduled Task starts the daemon at logon; `git status`, `pnpm install`, and `cargo build` work in `/root/workspace`; `sync setup` succeeds.
