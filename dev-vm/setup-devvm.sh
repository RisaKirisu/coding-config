#!/usr/bin/env bash
set -euo pipefail

DEVVM_HOME="${DEVVM_HOME:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
FRP_VERSION=0.71.0
FRPS_BIN="$HOME/.local/bin/frps"
INSTALL_SERVICE=0
SKIP_IMAGE=0
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
        --skip-image|--no-image-build)
            SKIP_IMAGE=1
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --service, --install-service   Install and start user service (systemd on Linux, launchd on macOS)"
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
if [[ "$SKIP_IMAGE" == "1" ]]; then
    echo "=== Skipping machine image build as requested ==="
elif [[ -f "$IMAGE" ]]; then
    echo "=== Machine image '$IMAGE' already exists, skipping build (run ./build.sh to rebuild) ==="
else
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
        "$HOME/.local/bin/devvm-daemon" service install --enable --start || echo "Warning: Service install exited with error"
    fi
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
echo "  - Control Daemon UI:    http://127.0.0.1:8100"
echo "  - Project Ingress URLs: http://<port>.<project-host>.devvm.localhost:8102"
echo ""
TAILSCALE_IP=""
if command -v tailscale >/dev/null 2>&1; then
    TAILSCALE_IP="$(tailscale ip -4 2>/dev/null || true)"
fi
if [[ -n "$TAILSCALE_IP" ]]; then
    echo "• Tailnet Access (Tailscale IP: $TAILSCALE_IP):"
    echo "  - Remote Control UI:    http://${TAILSCALE_IP}:8100"
    echo "  - Remote Project URLs:  http://<port>.<project-host>.devvm.internal:8102"
else
    echo "• Tailnet Access:"
    echo "  - Tailscale not detected or not connected. Once connected:"
    echo "  - Remote Control UI:    http://<tailscale-ip>:8100"
    echo "  - Remote Project URLs:  http://<port>.<project-host>.devvm.internal:8102"
fi
echo ""
echo "• One-Time Privileged DNS Setup (Optional for wildcard *.devvm.internal):"
echo "  - Helper script:        sudo ./scripts/setup-dns.sh"
echo "  - View instructions:    devvm-daemon dns setup"
echo ""
echo "• Sync Store Setup (Optional for portable DSH state sync to VPS):"
echo "  - Configure sync:       devvm-daemon sync setup --help"
echo ""
echo "• Note on State:"
echo "  - Existing shared DSH state and existing DevVMs are left untouched."
echo "  - No automatic migration is performed."
echo "=================================================="
