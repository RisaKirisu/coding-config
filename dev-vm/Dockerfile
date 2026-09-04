# syntax=docker/dockerfile:1
FROM python:3.14-slim-trixie

ARG NODE_VERSION=24.19.0
ARG NVM_VERSION=0.40.6
ARG FRP_VERSION=0.71.0
ARG CADDY_VERSION=2.11.4
ARG DSH_VERSION=0.1.2-rc.1

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/root/.cargo \
    NVM_DIR=/root/.nvm \
    RUSTUP_HOME=/root/.rustup \
    PATH=/root/.cargo/bin:/usr/local/bin:/root/.local/bin:$PATH \
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
    gfortran \
    libopenblas-dev \
    liblapack-dev \
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

RUN npm install --global pnpm && pnpm --version

RUN curl -fsSLo /tmp/install-nvm.sh \
        "https://raw.githubusercontent.com/nvm-sh/nvm/v${NVM_VERSION}/install.sh" \
    && PROFILE=/root/.bashrc bash /tmp/install-nvm.sh \
    && rm /tmp/install-nvm.sh \
    && bash -c '. "$NVM_DIR/nvm.sh" && nvm --version'

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

# ---------------------------------------------------------------------------
# Dev port access
# ---------------------------------------------------------------------------
# HTTP ingress for development servers. frpc provides the host tunnel and
# Caddy forwards each <port>.<project>.devvm.localhost request to localhost:<port>.
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
    amd64|arm64) ;; \
    *) echo "Unsupported architecture: $arch"; exit 1 ;; \
    esac; \
    tmpdir="$(mktemp -d)"; \
    curl -fsSL "https://github.com/fatedier/frp/releases/download/v${FRP_VERSION}/frp_${FRP_VERSION}_linux_${arch}.tar.gz" \
        | tar -xz -C "$tmpdir"; \
    install -m 0755 "$tmpdir/frp_${FRP_VERSION}_linux_${arch}/frpc" /usr/local/bin/frpc; \
    curl -fsSL "https://github.com/caddyserver/caddy/releases/download/v${CADDY_VERSION}/caddy_${CADDY_VERSION}_linux_${arch}.tar.gz" \
        | tar -xz -C "$tmpdir"; \
    install -m 0755 "$tmpdir/caddy" /usr/local/bin/caddy; \
    rm -rf "$tmpdir"

COPY scripts/Caddyfile /etc/devvm/Caddyfile
COPY scripts/frpc.toml /etc/devvm/frpc.toml
COPY --chmod=0755 scripts/devvm-ingress /usr/local/bin/devvm-ingress
COPY --chmod=0755 scripts/devvm-sync-startup /usr/local/bin/devvm-sync-startup
# ---------------------------------------------------------------------------
# Python
# ---------------------------------------------------------------------------
RUN python -m pip install --no-cache-dir --upgrade \
    pip \
    setuptools \
    wheel

# Install uv for other python versions
RUN curl -LsSf https://astral.sh/uv/install.sh | sh

# ---------------------------------------------------------------------------
# Coding Agents
# ---------------------------------------------------------------------------
# Install Deepseek Harness
# Extra heap space needed for dsh installation
ENV NODE_OPTIONS='--max-old-space-size=8192'
RUN npm install -g "@deepseek-ai/dsh@${DSH_VERSION}"
# Patch to allow dsh web to be accessed by host
COPY patches/deepseek-harness-localhost-subdomains.patch /tmp/deepseek-harness-localhost-subdomains.patch
RUN set -eux; \
    package_dir=/usr/local/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/dsh-client-connection; \
    client_target="$package_dir/lib/client.js"; \
    server_target="$package_dir/lib/index.js"; \
    printf '%s  %s\n' '1b8e76d7fb14c9305e6d43ab03bd908328341537af05c4159747d6da33308257' "$client_target" | sha256sum --check -; \
    printf '%s  %s\n' '91b8e7b90c5189a5dc0f4eafc4a62494eef9b5a28ded0715146ddf1a31126c8e' "$server_target" | sha256sum --check -; \
    patch --batch --forward --strip=1 --directory="$package_dir" < /tmp/deepseek-harness-localhost-subdomains.patch; \
    grep -Fq 'hostname.endsWith(".localhost")' "$client_target"; \
    grep -Fq 'hostname.endsWith(".localhost")' "$server_target"; \
    rm /tmp/deepseek-harness-localhost-subdomains.patch

# Install OpenCode
RUN npm install -g opencode-ai
ENV OPENCODE_EXPERIMENTAL_WEBSOCKETS=true \
    OPENCODE_ENABLE_EXA=1 \
    OPENCODE_EXPERIMENTAL_LSP_TOOL=1 \
    OPENCODE_EXPERIMENTAL_SCOUT=1 \
    OPENCODE_EXPERIMENTAL_PLAN_MODE=1

# Silence git safe.directory warning
RUN git config --global --add safe.directory "/root/*"

WORKDIR /root

CMD ["/bin/bash"]
