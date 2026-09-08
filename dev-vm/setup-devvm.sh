#!/usr/bin/env bash
set -euo pipefail

DEVVM_HOME="${DEVVM_HOME:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
FRP_VERSION=0.71.0
FRPS_BIN="$HOME/.local/bin/frps"
INSTALL_SERVICE=0
SKIP_IMAGE=0
REMOTE=0
REMOTE_DOMAIN=""
REMOTE_IP=""
SMOLVM_INSTALLER_URL="https://smolmachines.com/install.sh"
SMOLVM_RELEASE_URL="https://github.com/smol-machines/smolvm/releases/latest"

installed_smolvm_version() {
    if [[ -f "$HOME/.smolvm/.version" ]]; then
        tr -d '[:space:]' < "$HOME/.smolvm/.version"
    elif command -v smolvm >/dev/null 2>&1; then
        smolvm --version 2>/dev/null \
            | sed -nE 's/.*v?([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' \
            | head -1
    fi
    return 0
}

latest_smolvm_version() {
    curl -fsSLI -o /dev/null -w '%{url_effective}' "$SMOLVM_RELEASE_URL" \
        | sed -nE 's#.*/releases/tag/v?([^/]+)$#\1#p'
}

# Parse command-line flags
while [[ $# -gt 0 ]]; do
    case "$1" in
        --service|--install-service)
            INSTALL_SERVICE=1
            shift
            ;;
        --remote)
            REMOTE=1
            shift
            ;;
        --remote-domain)
            REMOTE_DOMAIN="$2"
            shift 2
            ;;
        --remote-ip)
            REMOTE_IP="$2"
            shift 2
            ;;
        --skip-image|--no-image-build)
            SKIP_IMAGE=1
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --service, --install-service   Install and start services"
            echo "  --remote                       Opt-in to host HTTPS remote access"
            echo "  --remote-domain <domain>       Override remote domain (requires --remote, default: risak.dev)"
            echo "  --remote-ip <ip>               Override remote host IP (requires --remote, default: 100.67.154.69)"
            echo "  --skip-image, --no-image-build Skip building microVM machine image"
            echo "  -h, --help                     Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

if [[ "$REMOTE" != "1" && (-n "$REMOTE_DOMAIN" || -n "$REMOTE_IP") ]]; then
    echo "Error: --remote-domain and --remote-ip require --remote" >&2
    exit 1
fi

if [[ "$REMOTE" == "1" ]]; then
    REMOTE_DOMAIN="${REMOTE_DOMAIN:-risak.dev}"
    REMOTE_IP="${REMOTE_IP:-100.67.154.69}"
fi

if [[ "$(realpath "$PWD")" != "$(realpath "$DEVVM_HOME")" ]]; then
    cd "$DEVVM_HOME"
fi

mkdir -p "$HOME/.local/bin"
mkdir -p "$DEVVM_HOME/root/.config/devvm"
install -d -m 0700 "$DEVVM_HOME/root/.ssh"

echo "=== Installing or upgrading smolvm ==="
LATEST_SMOLVM_VERSION="$(latest_smolvm_version)"
if [[ -z "$LATEST_SMOLVM_VERSION" ]]; then
    echo "Failed to determine the latest smolvm release." >&2
    exit 1
fi
CURRENT_SMOLVM_VERSION="$(installed_smolvm_version)"
if [[ "$CURRENT_SMOLVM_VERSION" == "$LATEST_SMOLVM_VERSION" ]]; then
    echo "smolvm $CURRENT_SMOLVM_VERSION is already up to date."
else
    if [[ -n "$CURRENT_SMOLVM_VERSION" ]]; then
        echo "Upgrading smolvm $CURRENT_SMOLVM_VERSION -> $LATEST_SMOLVM_VERSION..."
    else
        echo "Installing smolvm $LATEST_SMOLVM_VERSION..."
    fi
    curl -fsSL "$SMOLVM_INSTALLER_URL" \
        | bash -s -- --version "$LATEST_SMOLVM_VERSION"
fi

case "$(uname -s)" in
    Linux)  FRP_OS=linux ;;
    Darwin)
        FRP_OS=darwin
        if ! command -v mkfs.ext4 >/dev/null 2>&1 \
            && [[ ! -x "$(brew --prefix 2>/dev/null || true)/opt/e2fsprogs/sbin/mkfs.ext4" ]]; then
            echo "mkfs.ext4 not found. Install it with: brew install e2fsprogs" >&2
            exit 1
        fi
        ;;
    *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64)        FRP_ARCH=amd64; DEVVM_ARCH=amd64 ;;
    aarch64|arm64) FRP_ARCH=arm64; DEVVM_ARCH=arm64 ;;
    *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

echo "=== Installing devvm HTTP ingress (frps) ==="
if [[ ! -x "$FRPS_BIN" ]]; then
    TMP_DIR="$(mktemp -d)"
    trap 'rm -rf "$TMP_DIR"' EXIT
    curl -fsSL \
        "https://github.com/fatedier/frp/releases/download/v${FRP_VERSION}/frp_${FRP_VERSION}_${FRP_OS}_${FRP_ARCH}.tar.gz" \
        | tar -xz -C "$TMP_DIR"
    install -m 0755 "$TMP_DIR/frp_${FRP_VERSION}_${FRP_OS}_${FRP_ARCH}/frps" "$FRPS_BIN"
    rm -rf "$TMP_DIR"
    trap - EXIT
    echo "Installed frps to $FRPS_BIN"
else
    echo "frps is already installed at $FRPS_BIN"
fi

if [[ ! -f smolvm.toml ]]; then
    echo "=== Creating smolvm.toml from smolvm.toml.example ==="
    cp smolvm.toml.example smolvm.toml
fi

IMAGE="rust-dev-opencode-$DEVVM_ARCH.tar"
STALE_INPUT=""
if [[ -f "$IMAGE" ]]; then
    STALE_INPUT="$(find Dockerfile scripts patches -newer "$IMAGE" -print -quit 2>/dev/null)"
fi
if [[ "$SKIP_IMAGE" == "1" ]]; then
    echo "=== Skipping machine image build as requested ==="
elif [[ -f "$IMAGE" && -z "$STALE_INPUT" ]]; then
    echo "=== Machine image '$IMAGE' is up to date, skipping build ==="
else
    if [[ -n "$STALE_INPUT" ]]; then
        echo "=== '$STALE_INPUT' is newer than '$IMAGE', rebuilding ==="
    fi
    echo "=== Building Machine Image ==="
    ./build.sh
fi

echo "=== Linking devvm CLI into $HOME/.local/bin ==="
ln -sfn "$DEVVM_HOME/devvm" "$HOME/.local/bin/devvm"

echo "=== Building and installing devvm-daemon ==="
if command -v cargo >/dev/null 2>&1; then
    cargo build --release
    BUILT_DAEMON="$DEVVM_HOME/target/release/devvm-daemon"
    INSTALLED_DAEMON="$HOME/.local/bin/devvm-daemon"
    if [[ -f "$INSTALLED_DAEMON" ]] && cmp -s "$BUILT_DAEMON" "$INSTALLED_DAEMON"; then
        echo "devvm-daemon is already up to date."
    else
        install -m 0755 "$BUILT_DAEMON" "$INSTALLED_DAEMON"
        echo "Installed or upgraded devvm-daemon at $INSTALLED_DAEMON"
    fi
else
    echo "Warning: cargo not found. Please install Rust to build devvm-daemon." >&2
fi

if [[ "$INSTALL_SERVICE" == "1" ]]; then
    echo "=== Installing user service ==="
    if [[ -x "$HOME/.local/bin/devvm-daemon" ]]; then
        local_service_args=(service install --enable)
        if [[ "$REMOTE" == "1" ]]; then
            local_service_args+=(--remote-domain "$REMOTE_DOMAIN")
        fi
        if [[ "$(uname -s)" == "Linux" ]]; then
            "$HOME/.local/bin/devvm-daemon" "${local_service_args[@]}"
            systemctl --user restart devvm-daemon.service
        else
            "$HOME/.local/bin/devvm-daemon" "${local_service_args[@]}" --start
        fi
    fi
fi

if [[ "$REMOTE" == "1" ]]; then
    echo "=== Host Caddy configuration (no file written) ==="
    # Caddy expands these environment variables in the single shipped configuration.
    printf 'Caddy service environment: REMOTE_DOMAIN=%s REMOTE_IP=%s REMOTE_DOMAIN_REGEXP=%s\n' \
        "$REMOTE_DOMAIN" "$REMOTE_IP" "${REMOTE_DOMAIN//./\\.}"
    cat "$DEVVM_HOME/scripts/Caddyfile.host"
fi

echo ""
echo "=================================================="
echo "           DevVM Setup Complete                   "
echo "=================================================="
echo ""
echo "• Unprivileged Operation:"
echo "  - devvm CLI:            $HOME/.local/bin/devvm"
echo "  - devvm-daemon binary:  $HOME/.local/bin/devvm-daemon"
echo "  - Run in foreground:    devvm-daemon serve"
echo "  - Manage user service:  devvm-daemon service {install|status|uninstall}"
echo ""
echo "• Local Access:"
echo "  - Control Daemon UI:    http://control.devvm.localhost:8100"
echo "  - Project Ingress URLs: http://<port>.<project-host>.devvm.localhost:8102"
echo ""
if [[ "$REMOTE" == "1" ]]; then
    echo "• Remote HTTPS Access (Domain: $REMOTE_DOMAIN, Host IP: $REMOTE_IP):"
    echo "  - Remote Control UI:    https://devvm.$REMOTE_DOMAIN"
    echo "  - Remote Project URLs:  https://<project-host>-<port>.$REMOTE_DOMAIN"
    echo "  - Host Caddy config:    $DEVVM_HOME/scripts/Caddyfile.host (printed above; no file generated)"
    echo ""
    echo "• Host Caddy Setup:"
    echo "  - Install Caddy with Cloudflare DNS module on host"
    echo "  - Configure CLOUDFLARE_API_TOKEN in environment file"
    echo "  - Add the printed site block to your host Caddy configuration"
else
    echo "• Remote Access:"
    echo "  - Run setup again with --remote to configure host HTTPS routing"
fi
echo ""
echo "• Sync Store Setup (Optional for portable DSH state sync to VPS):"
echo "  - Configure sync:       devvm-daemon sync setup --help"
echo ""
echo "• Note on State:"
echo "  - Existing shared DSH state and existing DevVMs are left untouched."
echo "  - No automatic migration is performed."
echo "=================================================="
