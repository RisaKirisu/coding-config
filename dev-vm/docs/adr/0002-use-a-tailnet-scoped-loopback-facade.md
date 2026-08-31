# Use a tailnet-scoped loopback facade

Ingress presents every proxied DevVM application with loopback authority while browsers use routed Project URLs, avoiding per-application trusted-host configuration and covering server-side localhost checks in future plugins. This deliberately weakens application-level DNS-rebinding defenses behind the proxy, so remote access is confined to the Tailnet Boundary; browser-side hostname checks still require client support, and applications that depend on their external Host for absolute URLs may later need an explicit transparent mode.
