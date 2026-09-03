# 11: Detect Windows Tailscale from WSL

**What to build:** When the Control Daemon runs in WSL2 mirrored networking and Tailscale runs on Windows, detect the Windows Tailscale IPv4 address and bind the daemon to it as well as loopback.

**Blocked by:** none

**Status:** claimed

**Why:** `detect_tailscale_ipv4` invoked only `tailscale`, but WSL exposes the Windows client as `tailscale.exe`. The generated systemd user unit also emitted `Environment=PATH=...` without quoting; Windows PATH entries containing spaces were truncated at `/mnt/c/Program`, hiding `/mnt/c/Program Files/Tailscale/tailscale.exe`. On the affected machine, `tailscale.exe ip -4` returns `100.67.154.69`, that address is present on mirrored `eth0`, and binding it from WSL succeeds.

## Acceptance criteria

- [x] Detection tries `tailscale.exe` when `tailscale` is unavailable or fails.
- [x] Generated systemd units preserve PATH entries containing spaces.
- [x] Tests fail if the production candidate list drops `tailscale.exe`, fallback stops after the first failed CLI, or systemd PATH quoting is removed.
- [x] `setup-devvm.sh --service` restarts an upgraded Control Daemon and installs, enables, and restarts `devvm-daemon-dns.service` on Linux/WSL when Tailscale is connected.
- [x] `devvm-daemon dns setup` explains only the automated setup and the one unavoidable Tailscale Admin split-DNS action; it no longer prints four platform-specific manual procedures.
- [x] A successful `tailscale.exe` fallback does not emit a false error for the absent Linux `tailscale` command.
- [ ] After setup, the daemon logs that it listens on both `127.0.0.1:8100` and the mirrored Tailscale IP.
- [ ] Direct DNS query from a tailnet device resolves `*.devvm.internal` to the mirrored Tailscale IP.
- [ ] A tailnet browser reaches a Project URL under `*.devvm.internal:8102`.

## Comments

The previous live acceptance test was insufficient: it bound a DNS server to a random loopback port and queried that socket directly, bypassing service installation and Tailscale split DNS. Setup behavior now has an integration test that runs `setup-devvm.sh --service` with executable command adapters, inspects the installed DNS unit, and verifies the Control Daemon and DNS service restart commands. Production mutations that skip DNS setup, skip either restart, restore the multi-platform checklist, or emit a false fallback error each fail a named test.
