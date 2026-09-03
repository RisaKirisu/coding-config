#!/usr/bin/env bash
set -euo pipefail

# One-time privileged DNS setup helper for DevVM wildcard domains (*.devvm.internal)
# On Linux this script grants the installed daemon port-53 capability and installs the
# wildcard DNS user service. Daily devvm CLI and Control Daemon operations stay unprivileged.

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
    for cli in tailscale tailscale.exe; do
        if command -v "$cli" >/dev/null 2>&1; then
            TAILSCALE_IP="$("$cli" ip -4 2>/dev/null || true)"
            [[ -n "$TAILSCALE_IP" ]] && break
        fi
    done
fi
if [[ -z "${TAILSCALE_IP:-}" ]]; then
    echo "Tailscale is not connected or its CLI is not on PATH." >&2
    exit 1
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
        if [[ ! -x "$DEVVM_DAEMON_BIN" ]]; then
            echo "Daemon binary is not executable: $DEVVM_DAEMON_BIN" >&2
            exit 1
        fi
        echo "Allowing the DNS service to bind port $PORT..."
        if command -v setcap >/dev/null 2>&1; then
            run_cmd "${SUDO[@]}" setcap 'cap_net_bind_service=+ep' "$DEVVM_DAEMON_BIN"
        elif [[ "$DRY_RUN" == "1" ]]; then
            echo "[DRY-RUN] setcap cap_net_bind_service=+ep $DEVVM_DAEMON_BIN"
        else
            echo "setcap is required to let the DNS service bind port $PORT." >&2
            exit 1
        fi

        UNIT_DIR="$HOME/.config/systemd/user"
        UNIT_FILE="$UNIT_DIR/devvm-daemon-dns.service"
        UNIT_CONTENT="[Unit]
Description=DevVM wildcard DNS for the tailnet
After=network-online.target

[Service]
Type=simple
ExecStart=\"$DEVVM_DAEMON_BIN\" dns --bind $TAILSCALE_IP:$PORT --ip $TAILSCALE_IP --domain $DOMAIN
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target"

        if [[ "$DRY_RUN" == "1" ]]; then
            echo "[DRY-RUN] Write $UNIT_FILE:"
            printf '%s\n' "$UNIT_CONTENT"
            echo "[DRY-RUN] systemctl --user daemon-reload"
            echo "[DRY-RUN] systemctl --user enable devvm-daemon-dns.service"
            echo "[DRY-RUN] systemctl --user restart devvm-daemon-dns.service"
        else
            install -d -m 0700 "$UNIT_DIR"
            printf '%s\n' "$UNIT_CONTENT" > "$UNIT_FILE"
            systemctl --user daemon-reload
            systemctl --user enable devvm-daemon-dns.service
            systemctl --user restart devvm-daemon-dns.service
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
if [[ "$OS" == "Linux" ]]; then
    echo "Wildcard DNS service installed and running."
fi
echo "One tailnet-admin action remains:"
echo "  Tailscale Admin Console -> DNS -> Add nameserver"
echo "  Nameserver: $TAILSCALE_IP"
echo "  Restrict to domain: $DOMAIN"
