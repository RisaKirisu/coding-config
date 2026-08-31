#!/usr/bin/env bash
set -euo pipefail

# One-time privileged DNS setup helper for DevVM wildcard domains (*.devvm.internal)
# This script configures port-53 capabilities and local split-DNS resolvers.
# Daily devvm CLI and devvm-daemon operations remain unprivileged.

PORT="${PORT:-53}"
DOMAIN="${DOMAIN:-devvm.internal}"
DEVVM_DAEMON_BIN="${DEVVM_DAEMON_BIN:-$HOME/.local/bin/devvm-daemon}"
DRY_RUN="${DRY_RUN:-0}"

# Parse flags
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --port)
            PORT="$2"
            shift 2
            ;;
        --domain)
            DOMAIN="$2"
            shift 2
            ;;
        --tailscale-ip)
            TAILSCALE_IP="$2"
            shift 2
            ;;
        --bin)
            DEVVM_DAEMON_BIN="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--dry-run] [--port <port>] [--domain <domain>] [--tailscale-ip <ip>] [--bin <path>]"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

if [[ -z "${TAILSCALE_IP:-}" ]]; then
    if command -v tailscale >/dev/null 2>&1; then
        TAILSCALE_IP="$(tailscale ip -4 2>/dev/null || true)"
    fi
    TAILSCALE_IP="${TAILSCALE_IP:-127.0.0.1}"
fi

echo "=== DevVM One-Time DNS Setup ==="
echo "Domain: *.$DOMAIN"
echo "Target IP: $TAILSCALE_IP"
echo "Port: $PORT"
echo "Daemon binary: $DEVVM_DAEMON_BIN"
echo ""

run_cmd() {
    if [[ "$DRY_RUN" == "1" ]]; then
        echo "[DRY-RUN] $*"
    else
        echo "Executing: $*"
        "$@"
    fi
}

SUDO=()
if [[ $EUID -ne 0 ]]; then
    SUDO=(sudo)
fi

OS="$(uname -s)"
case "$OS" in
    Linux)
        echo "Configuring Linux DNS resolution..."
        
        # 1. Allow devvm-daemon to bind to port 53 without root if setcap is available
        if [[ -f "$DEVVM_DAEMON_BIN" ]] && command -v setcap >/dev/null 2>&1; then
            echo "Setting cap_net_bind_service on $DEVVM_DAEMON_BIN..."
            run_cmd "${SUDO[@]}" setcap 'cap_net_bind_service=+ep' "$DEVVM_DAEMON_BIN" || echo "Warning: setcap failed or not permitted."
        fi

        # 2. Configure systemd-resolved if available
        if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet systemd-resolved 2>/dev/null; then
            echo "Configuring systemd-resolved for $DOMAIN..."
            CONF_DIR="/etc/systemd/resolved.conf.d"
            CONF_FILE="$CONF_DIR/devvm.conf"
            
            if [[ "$DRY_RUN" == "1" ]]; then
                echo "[DRY-RUN] mkdir -p $CONF_DIR"
                echo "[DRY-RUN] Write $CONF_FILE with DNS=$TAILSCALE_IP:$PORT Domains=~$DOMAIN"
                echo "[DRY-RUN] systemctl restart systemd-resolved"
            else
                "${SUDO[@]}" mkdir -p "$CONF_DIR"
                cat <<EOF | "${SUDO[@]}" tee "$CONF_FILE" >/dev/null
[Resolve]
DNS=$TAILSCALE_IP:$PORT
Domains=~$DOMAIN
EOF
                echo "Restarting systemd-resolved..."
                "${SUDO[@]}" systemctl restart systemd-resolved || echo "Warning: failed to restart systemd-resolved"
            fi
        else
            echo "systemd-resolved not active. You can route DNS queries to $TAILSCALE_IP:$PORT via NetworkManager or local dnsmasq."
        fi
        ;;

    Darwin)
        echo "Configuring macOS /etc/resolver for $DOMAIN..."
        RESOLVER_DIR="/etc/resolver"
        RESOLVER_FILE="$RESOLVER_DIR/$DOMAIN"

        if [[ "$DRY_RUN" == "1" ]]; then
            echo "[DRY-RUN] mkdir -p $RESOLVER_DIR"
            echo "[DRY-RUN] Write $RESOLVER_FILE with nameserver $TAILSCALE_IP"
        else
            "${SUDO[@]}" mkdir -p "$RESOLVER_DIR"
            if [[ "$PORT" == "53" ]]; then
                cat <<EOF | "${SUDO[@]}" tee "$RESOLVER_FILE" >/dev/null
nameserver $TAILSCALE_IP
EOF
            else
                cat <<EOF | "${SUDO[@]}" tee "$RESOLVER_FILE" >/dev/null
nameserver $TAILSCALE_IP
port $PORT
EOF
            fi
            echo "macOS resolver written to $RESOLVER_FILE"
        fi
        ;;

    *)
        echo "Unsupported OS for automatic resolver configuration: $OS"
        ;;
esac

echo ""
echo "=== Tailscale Split DNS (Optional for Remote Access) ==="
echo "To resolve *.$DOMAIN from other devices on your Tailnet:"
echo "  1. Open Tailscale Admin Console -> DNS -> Nameservers"
echo "  2. Click 'Add Nameserver' -> 'Custom Nameserver'"
echo "  3. Restrict to domain: '$DOMAIN'"
echo "  4. Set Nameserver IP: '$TAILSCALE_IP'"
echo "  5. Save configuration"
echo ""
echo "DNS setup complete. Run 'devvm-daemon dns --ip $TAILSCALE_IP' to start serving DNS records."
