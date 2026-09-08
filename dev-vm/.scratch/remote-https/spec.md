# Replace private DNS with host HTTPS over Tailscale

Status: implemented; live HTTPS deployment acceptance pending

Canonical design: [remote access](../../docs/remote-access.md).

Replace the custom devvm.internal DNS service with public Cloudflare DNS records and a private host Caddy HTTPS listener. Default remote domain: risak.dev. Default remote IP: 100.67.154.69. Remote setup is opt-in; local-only installations require no Tailscale or Cloudflare setup. Explicit setup-devvm.sh arguments can override the remote domain and IP. No automatic network discovery is allowed.

Implementation: [01 — Replace private DNS and remote routing](issues/01-replace-private-dns.md).

The host operator installs Caddy with the Cloudflare DNS module and supplies a restricted API token. Repository work includes one standalone host Caddyfile, printed by setup and loaded directly by tests, daemon URL changes, obsolete DNS removal, tests, and migration documentation. Publishing DNS, supplying credentials, and restarting the actual main workstation services are outside ticket execution unless separately authorized and accessible.
