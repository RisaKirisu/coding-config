# DeepSeek Harness localhost findings

## Scope

This investigates DeepSeek Harness (DSH) `0.1.1-rc.2` served from a dev VM at:

```text
http://3080.<project>.devvm.localhost
```

Related upstream discussion: <https://github.com/deepseek-ai/deepseek-harness/discussions/894>

The live test machine was `dev-xiaobright-modeltest-3f4561e7`. The resulting
patch is applied during the `devvm` image build.

## Findings

Harness has separate browser-side and server-side loopback checks.

### Browser check

`@deepseek-ai/dsh-client-connection/lib/client.js` calculates
`ctx.connection.isLoopback` from `window.location.hostname`. Its
`isLoopbackHostname()` accepts only:

- `localhost`
- `[::1]`
- IPv4 addresses in `127/8`

When the browser opens `3080.<project>.devvm.localhost`, the client reports
`isLoopback: false`. The UI then suppresses privileged settings requests before
they reach the server. The Models page reports that settings are unavailable in
this browser.

Caddy cannot fix this check because a reverse proxy cannot change
`window.location.hostname` in already-running browser JavaScript.

### Server check

`@deepseek-ai/dsh-client-connection/lib/index.js` applies the same
`isLoopbackHostname()` predicate to the HTTP `Host` header. Normal API routes
can accept authorities supplied through `dsh web --trusted-host`, but privileged
methods such as `settings.describe` call `isTrustedApiRequest(request, [])`.
The empty trusted-host list deliberately requires loopback classification.

After patching only the browser bundle, the request was sent but returned:

```text
transport failure for /api/settings.describe: HTTP 403
```

This proves the browser patch passed and exposed the independent server check.

### General classification

The narrow initial workaround accepted only `.devvm.localhost`. The more general
patch accepts any hostname ending in `.localhost` on both sides:

```js
hostname === "localhost" || hostname.endsWith(".localhost")
```

This matches the special-use `localhost` namespace rather than coupling Harness
to devvm. Browser and resolver implementations treat names under `.localhost`
as loopback. Existing same-origin, `Sec-Fetch-Site`, and Host/Origin comparison
checks remain in place.

With both bundles patched, a temporary DSH instance on port 3081 received a
request through unmodified generic Caddy ingress with:

```text
Host: 3081.xiaobright-modeltest-3f4561e7.devvm.localhost
Origin: http://3081.xiaobright-modeltest-3f4561e7.devvm.localhost
Sec-Fetch-Site: same-origin
```

`POST /api/settings.describe` returned HTTP 200.

## Patch

This patch targets built files from DSH `0.1.1-rc.2`:

```diff
--- a/lib/client.js
+++ b/lib/client.js
@@ -10244,10 +10244,10 @@
 		/**
 		* Whether a normalized URL hostname names the local loopback authority.
 		* @param hostname - WHATWG URL hostname (IPv6 literals retain brackets).
-		* @returns true for localhost, IPv6 loopback, or any IPv4 address in 127/8.
+		* @returns true for localhost names, IPv6 loopback, or any IPv4 address in 127/8.
 		*/
 		function isLoopbackHostname(hostname) {
-			if (hostname === "localhost" || hostname === "[::1]") return true;
+			if (hostname === "localhost" || hostname.endsWith(".localhost") || hostname === "[::1]") return true;
 			const parts = hostname.split(".");
 			return parts.length === 4 && parts[0] === "127" && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
 		}
--- a/lib/index.js
+++ b/lib/index.js
@@ -95,10 +95,10 @@
 /**
 * Whether a normalized URL hostname names the local loopback authority.
 * @param hostname - WHATWG URL hostname (IPv6 literals retain brackets).
-* @returns true for localhost, IPv6 loopback, or any IPv4 address in 127/8.
+* @returns true for localhost names, IPv6 loopback, or any IPv4 address in 127/8.
 */
 function isLoopbackHostname(hostname) {
-	if (hostname === "localhost" || hostname === "[::1]") return true;
+	if (hostname === "localhost" || hostname.endsWith(".localhost") || hostname === "[::1]") return true;
 	const parts = hostname.split(".");
 	return parts.length === 4 && parts[0] === "127" && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
 }
```

The live machine stores it at:

```text
/usr/local/share/devvm-patches/deepseek-harness-localhost-subdomains.patch
```

## Build integration

`Dockerfile` applies the patch immediately after installing DSH. It verifies
the SHA-256 hash of each target bundle first, then applies
`patches/deepseek-harness-localhost-subdomains.patch`. These checks depend on
target content, not the DSH package version, so a package update with unchanged
bundles still receives the patch. A changed or partially patched bundle fails
the image build for review instead of receiving a stale patch.

The original live-machine script was:

```bash
#!/usr/bin/env bash
set -euo pipefail

package_dir="${DSH_CLIENT_CONNECTION_DIR:-/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/dsh-client-connection}"
client_target="$package_dir/lib/client.js"
server_target="$package_dir/lib/index.js"
patch_file="/usr/local/share/devvm-patches/deepseek-harness-localhost-subdomains.patch"
marker='hostname.endsWith(".localhost")'
original='if (hostname === "localhost" || hostname === "[::1]") return true;'

if grep -Fq "$marker" "$client_target" && grep -Fq "$marker" "$server_target"; then
    echo "DeepSeek Harness localhost-subdomain patch already applied"
    exit 0
fi

if ! grep -Fq "$original" "$client_target" || ! grep -Fq "$original" "$server_target"; then
    echo "DeepSeek Harness bundle changed or is partially patched; refusing stale patch" >&2
    exit 1
fi

patch --batch --forward --strip=1 --directory="$package_dir" < "$patch_file"
grep -Fq "$marker" "$client_target"
grep -Fq "$marker" "$server_target"
echo "Applied DeepSeek Harness localhost-subdomain patch"
```

The image build replaces this runtime step. Recheck the upstream bundles,
regenerate the patch, and update expected hashes when either target changes.

## Caddy alternative

Caddy can satisfy only the server-side check by rewriting the exact same-origin
authority before proxying:

```caddyfile
@deepseek_harness header Host 3080.<project>.devvm.localhost
reverse_proxy @deepseek_harness 127.0.0.1:3080 {
	header_up Host localhost:3080
	header_up Origin "^http://3080\.<project>\.devvm\.localhost$" "http://localhost:3080"
}
```

This made the server see loopback Host and Origin values, but it could not make
the browser classify `window.location.hostname` as loopback. A browser patch was
still required.

Broad Caddy rewriting is not recommended. It changes application-visible
authority, may alter redirects or cookies, and can weaken an application's own
Host/Origin defenses. If retained, match one expected host and rewrite only its
exact same-origin Origin.

The live machine's Caddy config has been restored to generic ingress with no
Harness-specific rewrite. A header-echo probe confirmed that it preserves the
original Host and Origin.

## Continuing the work

1. Add upstream tests covering `localhost`, subdomains such as `app.localhost`,
   `[::1]`, `127/8`, and non-localhost lookalikes such as `notlocalhost`.
2. Remove the built-bundle patch once upstream classifies `.localhost`
   subdomains consistently in both browser and server source.

For devvm ingress, the browser authority is the hostname without a scheme or
TCP port:

```text
3080.<project>.devvm.localhost
```

The browser URL uses HTTP port 80. The leading `3080` is part of the hostname
that Caddy uses to select guest port 3080; it is not an external `:3080` port.
