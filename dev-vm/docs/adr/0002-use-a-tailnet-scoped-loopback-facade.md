# Use a tailnet-scoped loopback facade

Ingress presents every proxied DevVM application with loopback authority while browsers use routed Project URLs, avoiding per-application trusted-host configuration and covering server-side localhost checks in future plugins. This deliberately weakens application-level DNS-rebinding defenses behind the proxy, so remote access is confined to the Tailnet Boundary; browser-side hostname checks still require client support, and applications that depend on their external Host for absolute URLs may later need an explicit transparent mode.

## DSH 0.1.2-rc.1 browser authentication amendment

DSH `0.1.2-rc.1` adds browser authentication that exchanges a launch URL query token for an authority-bound `HttpOnly; SameSite=Strict` cookie and immediately redirects to a token-free `/`. The redirect succeeds only when the navigation into the Project URL is same-site. Opening the Control Daemon at `127.0.0.1:8100`, bare `localhost:8100`, or a raw Tailscale IP makes a click into `*.devvm.localhost` or `*.<remote-domain>` cross-site: the browser stores the Strict cookie but withholds it on DSH's redirect, which then returns HTTP 401. Pasting the same token URL directly works because the navigation has no cross-site initiator.

The canonical Control Daemon URLs are therefore `http://control.devvm.localhost:8100` for local use and `https://devvm.<remote-domain>` (default: `https://devvm.risak.dev`) for remote tailnet use. Each shares a site with its corresponding Project URLs (`*.devvm.localhost` and `*.<remote-domain>`), so DSH's unmodified Strict cookie is sent on the redirect. The local hostname works through the standard `.localhost` loopback namespace. The remote HTTPS hostname routes through host Caddy on port 443 of the configured Tailscale IP to the loopback Control Daemon on port 8100; Project URLs traverse host Caddy -> FRP on port 8102 -> guest Caddy Loopback Facade.

This amendment is specific to the authentication flow observed and browser-tested against DSH `0.1.2-rc.1`. Re-evaluate the canonical-host requirement when upgrading DSH if its cookie policy or token exchange changes.
