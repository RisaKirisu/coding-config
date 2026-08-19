#!/bin/bash

set -euo pipefail

ARCH=$(uname -m)

case "$ARCH" in
  x86_64) ARCH=amd64 ;;
  aarch64|arm64) ARCH=arm64 ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

docker build -t rust-dev-smolvm-opencode-$ARCH .
docker save rust-dev-smolvm-opencode-$ARCH -o rust-dev-opencode-$ARCH.tar