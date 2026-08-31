# 02: Expose local and tailnet Project URLs

**What to build:** Make DSH and arbitrary guest HTTP ports reachable through local and tailnet Project URLs while presenting loopback authority to applications through the global Loopback Facade.

**Blocked by:** 01: Build the Control Daemon and manage Project runtimes

**Status:** resolved

- [x] Local Project URLs retain the existing localhost-subdomain routing pattern.
- [x] The central UI provides Open Port, which accepts a guest port and returns a Project URL without managing the guest process.
- [x] Caddy applies the Loopback Facade to every proxied guest port after selecting the port from the browser-facing Project URL.
- [x] Application-facing Host and matching same-origin Origin values are presented as loopback authority.
- [x] Better Sidebar works through a Project URL without manually passing a trusted host.
- [x] DSH browser-side localhost recognition supports the agreed local and tailnet Project URL suffixes.
- [x] The Control Daemon and Project URLs are reachable through Tailscale without an additional application login.
- [x] Remote Project URLs use wildcard `*.devvm.internal` names that resolve to the active workstation's Tailscale address.
- [x] The same Rust binary provides a separate DNS mode for wildcard private-name resolution.
- [x] DNS mode is separable from the unprivileged Control Daemon so only DNS setup requires port-53 privilege.
- [x] The VPS does not participate in DNS.
- [x] Real-Caddy integration tests verify Project URL routing and Loopback Facade behavior against a header-echo application.
- [x] DNS integration tests verify wildcard private-name resolution to the workstation address.
- [x] FRP receives a focused configuration and transport smoke test rather than tests of FRP internals.
