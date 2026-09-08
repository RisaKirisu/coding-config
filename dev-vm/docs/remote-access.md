# Remote access through public DNS and Tailscale

## Setup

Most workstations use local access only:

```sh
./setup-devvm.sh --service
```

On the main workstation:

```sh
./setup-devvm.sh --service --remote
# Optional overrides:
./setup-devvm.sh --service --remote --remote-domain risak.dev --remote-ip 100.67.154.69
```

Remote defaults are `risak.dev` and `100.67.154.69`. Setup installs the daemon's remote-domain argument and prints the host Caddy site block and environment settings. It does not create or overwrite a host Caddyfile. There is no automatic Tailscale detection. Without `--service`, setup prints configuration but does not persist daemon startup options; use `devvm-daemon serve --remote-domain risak.dev` when launching manually.

## DNS and routing

Cloudflare DNS-only A records for `devvm.risak.dev` and `*.risak.dev` point to `100.67.154.69`. Specific records override the wildcard. Only authorized Tailscale clients can reach this address.

- Control: `https://devvm.risak.dev` -> host Caddy -> loopback `8100`.
- Project: `https://<project-host>-<port>.risak.dev` -> host Caddy -> FRP on loopback `8102` -> guest Caddy -> application.
- Local: `http://control.devvm.localhost:8100` and `http://<port>.<project-host>.devvm.localhost:8102`.

Project hosts normally contain the project name and existing eight-character path hash. If the name exceeds 48 characters, use just that hash. The daemon and `devvm` assign the same identity before URL generation and FRP registration; URL formatting never renames it. VM lifecycle names are independent of this routing label.

Host Caddy translates matching Host and Origin headers into internal `.devvm.localhost` routing labels. Those labels do not require remote DNS resolution. Unrelated origins remain unchanged so applications can reject them. Unknown host shapes are rejected. Keep remote Control and Project URLs on HTTPS under the same parent domain for DSH's SameSite=Strict cookie flow.

## Host Caddy

The only host routing configuration is [scripts/Caddyfile.host](../scripts/Caddyfile.host). Add its site block to the existing host Caddy configuration; do not run a second listener competing for the same address. The Rust daemon does not generate Caddy configuration.

Use Caddy with the Cloudflare DNS module. Verify `caddy list-modules` includes `dns.providers.cloudflare`. The official download builder or `xcaddy build --with github.com/caddy-dns/cloudflare` can provide it.

Configure the Caddy service environment with `CLOUDFLARE_API_TOKEN`, restricted to the selected zone with DNS Edit and Zone Read. Store it outside the repository in a protected environment file. For overrides, also set `REMOTE_DOMAIN`, `REMOTE_IP`, and `REMOTE_DOMAIN_REGEXP` to the values printed by setup; the last value is the domain with dots escaped for regular expressions. Defaults need no override variables.

Use the normal Caddy service account with permission to bind port 443 and persistent certificate storage. Do not run Caddy as root merely for port binding. Its DNS-01 certificate validation requires outbound access and DNS control, not public inbound ports. Only host Caddy receives DNS credentials.

The listener's IP must be available where Caddy runs, and loopback ports 8100/8102 must reach the host services. In WSL, verify that placement against the actual Windows/WSL networking setup.

Reference: https://caddyserver.com/docs/automatic-https#dns-challenge

## Migration and verification

Stop and disable the old `devvm-daemon-dns.service`, remove its user service unit, and reload the service manager. Remove the old `devvm.internal` restricted nameserver from Tailscale. Remove only DevVM-created resolver files and the obsolete DNS bind-port capability on the daemon binary where present. Do not change other resolver configuration or Caddy's permissions.

For installations that previously used name truncation, long-name Project URLs now use the hash-only host. Restart affected guest ingress so FRP registers the new routing label; do not rename or delete existing VMs as part of DNS cleanup.

Offline integration tests read the shipped host Caddyfile and replace only TLS/listener settings and upstream ports. They verify routing, Origin translation, DSH server authentication and WebSocket upgrade. HTTP cookie replay is not browser SameSite enforcement, and chunked HTML is not a full interactive streaming test. Live HTTPS browser login, certificate issuance/renewal, and access from a second Tailscale device remain deployment checks.

The VPS remains dedicated to Session Sync. No public gateway or additional authentication system is introduced.
