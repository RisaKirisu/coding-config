# 01 — Replace daemon private DNS with opt-in host HTTPS routing

Type: task
Status: implemented; live HTTPS deployment acceptance pending

## Problem and contract

The custom devvm.internal resolver was unreliable. Replace it with Cloudflare DNS-only records and host Caddy over Tailscale, using the contract in [docs/remote-access.md](../../../docs/remote-access.md).

Local setup stays local-only. Remote setup is opt-in, defaults to risak.dev and 100.67.154.69, and accepts explicit domain/IP overrides. The daemon has one loopback listener by default and one optional remote domain. No IP discovery, DNS server, public tunnel, or extra authentication system is required.

Host configuration has one source, scripts/Caddyfile.host. Setup prints that file and environment settings; it does not generate a Rust-backed configuration or overwrite host files. Remote URLs use flat project names. Names exceeding 48 characters use their existing eight-character path hash, assigned consistently in devvm and compute_project_host. URL formatting must not independently rename routing identities.

Preserve FRP, guest Caddy, local URLs, DSH authentication, and Session Sync. Document host installation and one-time private-DNS cleanup without deploying from the guest.

## Acceptance

- Default and overridden setup behavior, local-only link omission, and remote API links are covered.
- Remove the DNS implementation, obsolete configuration, and automatic detection.
- Tests load the actual host Caddyfile and exercise Host/Origin translation through FRP and guest Caddy. Unknown host shapes are rejected; unrelated origins are not rewritten into trusted origins.
- Lifecycle tests retain guest path isolation and cover non-interactive start, stop/relaunch, daemon-restart status recovery, and Project Logs.
- Browser HTTPS login and SameSite enforcement, live DNS-01 issuance/renewal, and second-device access are separate deployment acceptance checks, not inferred from HTTP cookie replay.

## Comments

The initial build introduced a test-only Rust Caddy generator, a shell heredoc, and a standalone Caddyfile. Its audits tested the Rust copy and overstated browser verification. The direct cleanup removed the generator, shell duplicate, redundant remote/tailnet config state, obsolete multi-listener logic/test, unused URL helpers, and independent URL rehashing/truncation. Documentation now describes a normal host Caddy service instead of a bespoke root service.

After cleanup, the complete Rust suite passed: 100 tests, one live acceptance test ignored. Mutating the Host rewrite in scripts/Caddyfile.host made the focused proxy test fail with "Project 1 failed to route through FRP chain"; the production rewrite was then restored. Live HTTPS deployment remains unverified.
