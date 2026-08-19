#!/bin/bash
set -euo pipefail

DEVVM_HOME="$HOME/coding-config/dev-vm"

if [[ "$(realpath "$PWD")" != "$(realpath "$DEVVM_HOME")" ]]; then
    cd "$DEVVM_HOME"
fi

if ! command -v smolvm >/dev/null 2>&1; then
    echo "Installing smolvm..."
    curl -sSL https://smolmachines.com/install.sh | bash
fi

echo "=== Building Machine Image ==="
./build.sh

echo "=== Installing devvm into $HOME/.local/bin"
mkdir -p "$HOME/.local/bin"
ln -sfn "$DEVVM_HOME/devvm" "$HOME/.local/bin/devvm"