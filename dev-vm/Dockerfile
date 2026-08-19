FROM python:3.14-slim-trixie

ARG NODE_VERSION=24.19.0

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/root/.cargo \
    RUSTUP_HOME=/root/.rustup \
    PATH=/root/.cargo/bin:/usr/local/bin:$PATH \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

# General development environment.
RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    curl \
    file \
    fd-find \
    gdb \
    git \
    jq \
    less \
    libclang-dev \
    libssl-dev \
    libsqlite3-dev \
    libzstd-dev \
    lld \
    llvm \
    make \
    nano \
    ninja-build \
    openssh-client \
    patch \
    pkg-config \
    procps \
    ripgrep \
    rsync \
    strace \
    tar \
    unzip \
    vim-tiny \
    wget \
    xz-utils \
    zip \
    zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Node.js + npm
#
# Node 24 is the current LTS line as of August 2026.
# Install directly from the official Node.js binaries rather than Debian's
# considerably older Node package.
# ---------------------------------------------------------------------------
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
    amd64) node_arch="x64" ;; \
    arm64) node_arch="arm64" ;; \
    armhf) node_arch="armv7l" ;; \
    *) echo "Unsupported architecture: $arch"; exit 1 ;; \
    esac; \
    filename="node-v${NODE_VERSION}-linux-${node_arch}.tar.xz"; \
    curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/${filename}"; \
    curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt"; \
    grep " ${filename}\$" SHASUMS256.txt | sha256sum -c -; \
    tar -xJf "${filename}" -C /usr/local --strip-components=1; \
    rm "${filename}" SHASUMS256.txt; \
    node --version; \
    npm --version

# ---------------------------------------------------------------------------
# GitHub CLI
# Official GitHub apt repository.
# ---------------------------------------------------------------------------
RUN mkdir -p -m 755 /etc/apt/keyrings \
    && wget -qO /etc/apt/keyrings/githubcli-archive-keyring.gpg \
    https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    && chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Rust
# ---------------------------------------------------------------------------
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup component add \
    clippy \
    rustfmt \
    rust-analyzer \
    rust-src

# cargo-binstall lets the sandbox cheaply add more Rust CLI tools later.
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    | bash

# Python development tooling.
RUN python -m pip install --no-cache-dir --upgrade \
    pip \
    setuptools \
    wheel \
    numpy \
    scipy \
    matplotlib \
    requests

# Install OpenCode
RUN curl -fsSL https://opencode.ai/install | bash
ENV OPENCODE_EXPERIMENTAL_WEBSOCKETS=true \
    OPENCODE_ENABLE_EXA=1 \
    OPENCODE_EXPERIMENTAL_LSP_TOOL=1 \
    OPENCODE_EXPERIMENTAL_SCOUT=1 \
    OPENCODE_EXPERIMENTAL_PLAN_MODE=1

WORKDIR /workspace

CMD ["/bin/bash"]