FROM mcr.microsoft.com/vscode/devcontainers/base:ubuntu-24.04

# Install system dependencies including Node.js for OpenAI Codex CLI,
# as well as build tools for potential native dependencies
RUN apt-get update && apt-get install -y \
  curl \
  git \
  sudo \
  xz-utils \
  ca-certificates \
  nodejs \
  npm \
  build-essential \
  pkg-config \
  && rm -rf /var/lib/apt/lists/*

# Install OpenAI Codex CLI globally
# https://github.com/openai/codex
RUN npm install -g @openai/codex

# Create vscode user if it doesn't exist
RUN if ! id -u vscode > /dev/null 2>&1; then \
        groupadd --gid 1000 vscode && \
        useradd --uid 1000 --gid vscode --shell /bin/bash --create-home vscode && \
        echo 'vscode ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers; \
    fi

# Switch to vscode user
USER vscode
WORKDIR /home/vscode

# Install Nix (single-user installation)
# Single-user installation is preferred in containers because:
# 1. Containers already provide process isolation, making multi-user security benefits redundant
# 2. No need for systemd daemon management complexity in containerized environments
# 3. Simpler permission model without group management overhead
# 4. Faster startup times and less resource consumption
# 5. Easier troubleshooting without daemon-related issues
# Reference: https://nixos.org/manual/nix/stable/installation/single-user.html
RUN curl -L https://nixos.org/nix/install | sh -s -- --no-daemon

# Set up Nix environment
ENV PATH="/home/vscode/.nix-profile/bin:${PATH}"
RUN echo 'source /home/vscode/.nix-profile/etc/profile.d/nix.sh' | sudo tee /etc/profile.d/nix.sh

# Pre-create Cargo directories to ensure proper permissions
# The cargo cache will be mounted as a volume, so we set up the structure
RUN mkdir -p /home/vscode/.cargo/{bin,registry,git}

# Configure Nix (See scripts/nix.conf for rationale for each setting)
RUN mkdir -p /home/vscode/.config/nix
COPY scripts/nix.conf /home/vscode/.config/nix/nix.conf

# Set up development utility scripts with proper permissions
RUN mkdir -p /home/vscode/.local/bin
COPY --chmod=755 scripts/setup-nix-substituters.sh /home/vscode/.local/bin/setup-nix-substituters
COPY --chmod=755 scripts/sync-cargo-caches.sh /home/vscode/.local/bin/sync-cargo-caches
COPY --chmod=755 scripts/efficient-copy.sh /home/vscode/.local/bin/efficient-copy

# Run Nix setup once during container creation (this is build-time appropriate)
# Cargo cache seeding is intentionally NOT run here - it should be done per-session
RUN /home/vscode/.local/bin/setup-nix-substituters

# Declare volumes AFTER all setup is complete to preserve build-time directory structure
# This ensures that Nix store directories and Cargo cache structure created during build
# are available as base content when volumes are mounted at runtime
VOLUME /nix/store
VOLUME /nix/host-store
VOLUME /host-cargo
VOLUME /home/vscode/.cargo

# Set working directory
WORKDIR /workspaces/agent-harbor

# Ensure proper permissions
USER root
RUN chown -R vscode:vscode /home/vscode
USER vscode
