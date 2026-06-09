# Minimal sandbox image for helpcore command execution.
FROM node:22-slim

# Install basic tools for code exploration and building.
RUN apt-get update && apt-get install -y \
    git \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Enable pnpm for web app tasks.
RUN corepack enable && corepack prepare pnpm@latest --activate

USER node
WORKDIR /workspace

# Trust the workspace for git operations.
RUN git config --global safe.directory /workspace
