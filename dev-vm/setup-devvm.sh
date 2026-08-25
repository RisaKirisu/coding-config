#!/bin/bash
set -euo pipefail

DEVVM_HOME="$HOME/coding-config/dev-vm"
FRP_VERSION=0.71.0
FRPS_BIN="$HOME/.local/bin/frps"

if [[ "$(realpath "$PWD")" != "$(realpath "$DEVVM_HOME")" ]]; then
    cd "$DEVVM_HOME"
fi

echo "Installing smolvm..."
curl -sSL https://smolmachines.com/install.sh | bash

case "$(uname -m)" in
    x86_64)  FRP_ARCH=amd64 ;;
    aarch64) FRP_ARCH=arm64 ;;
    *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

echo "=== Installing devvm HTTP ingress ==="
mkdir -p "$HOME/.local/bin"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
curl -fsSL \
    "https://github.com/fatedier/frp/releases/download/v${FRP_VERSION}/frp_${FRP_VERSION}_linux_${FRP_ARCH}.tar.gz" \
    | tar -xz -C "$TMP_DIR"
install -m 0755 "$TMP_DIR/frp_${FRP_VERSION}_linux_${FRP_ARCH}/frps" "$FRPS_BIN"
sudo setcap cap_net_bind_service=+ep "$FRPS_BIN"

echo "=== Building Machine Image ==="
./build.sh

echo "=== Installing devvm into $HOME/.local/bin"
ln -sfn "$DEVVM_HOME/devvm" "$HOME/.local/bin/devvm"
