# 04: Install the complete system on Linux and macOS

**What to build:** Integrate the completed Control Daemon, Project URL ingress, wildcard DNS, and optional synchronization into one setup and runtime experience on Linux and macOS.

**Blocked by:** 02: Expose local and tailnet Project URLs; 03: Provision and synchronize Portable DSH State

**Status:** resolved

- [x] One setup entry point installs or configures the existing DevVM dependencies plus the Control Daemon, DSH plugin, ingress changes, DNS mode, and synchronization setup integration.
- [x] Foreground Control Daemon operation remains available without enabling a service.
- [x] Linux offers optional systemd user integration.
- [x] macOS offers optional launchd user integration.
- [x] One-time privileged DNS setup is separate from ordinary unprivileged daemon and CLI use.
- [x] Sync Store configuration remains optional during setup.
- [x] Local and Tailscale access configuration is integrated into the setup flow.
- [x] Setup is verified on supported Linux and macOS hosts.
- [x] Existing shared DSH data and existing DevVMs are left untouched; no automatic migration occurs.
