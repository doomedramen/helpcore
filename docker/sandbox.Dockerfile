# Sandbox image for the assistant's workspace tools.
# Build with: make sandbox-image
#
# Requirements for any replacement image: /bin/sh and coreutils `timeout`
# (used to enforce per-command time limits). Everything else is convenience
# tooling for working on code inside /workspace.

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    openssh-client \
    curl \
    wget \
    ca-certificates \
    python3 \
    python3-pip \
    python3-venv \
    nodejs \
    npm \
    sqlite3 \
    jq \
    ripgrep \
    fd-find \
    build-essential \
    pkg-config \
    libssl-dev \
    unzip \
    zip \
    less \
    procps \
    && rm -rf /var/lib/apt/lists/* \
    && ln -s "$(command -v fdfind)" /usr/local/bin/fd

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --component clippy --component rustfmt
ENV PATH="/root/.cargo/bin:${PATH}"

# Downloadable caches (cargo registry, pip, npm) are redirected to the
# persistent /workspace volume at runtime so they survive container
# recreation; see SESSION_ENV in the server's sandbox module.

RUN mkdir -p /workspace
WORKDIR /workspace
