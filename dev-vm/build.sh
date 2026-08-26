#!/bin/bash

set -euo pipefail

if command -v docker >/dev/null 2>&1; then
  CONTAINER_RUNTIME=docker
elif command -v podman >/dev/null 2>&1; then
  CONTAINER_RUNTIME=podman
else
  echo "Docker or Podman is required to build the machine image" >&2
  exit 1
fi

ARCH=$(uname -m)

case "$ARCH" in
  x86_64) ARCH=amd64 ;;
  aarch64|arm64) ARCH=arm64 ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

"$CONTAINER_RUNTIME" build -t rust-dev-smolvm-opencode-$ARCH .
"$CONTAINER_RUNTIME" save -o rust-dev-opencode-$ARCH.tar rust-dev-smolvm-opencode-$ARCH
