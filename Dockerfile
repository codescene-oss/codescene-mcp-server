# ── Stage 1: Build Rust binary ────────────────────────────────────────────────
FROM rust:1-bookworm AS builder

# Version injected at build time (e.g., "MCP-1.2.3"); passed to build.rs
# via the CS_MCP_VERSION env var to embed in the binary.
ARG VERSION=dev

WORKDIR /build

# Copy only what the build needs (keeps layer cache friendly)
COPY Cargo.toml Cargo.lock build.rs cli-checksums.sha256 ./
COPY src/ src/

RUN CS_MCP_VERSION="${VERSION}" cargo build --release

# ── Stage 2: Minimal runtime image ───────────────────────────────────────────
FROM debian:bookworm-slim

# git   – needed by several MCP tools that inspect repositories
# curl  – used to fetch the CodeScene CLI installer
# ca-certificates – TLS for outbound HTTPS calls
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git curl unzip \
    && rm -rf /var/lib/apt/lists/*

# Allow git to operate on bind-mounted repos owned by a different user.
# Written to the *system* gitconfig so it applies regardless of which
# UID the container runs as (the default mcp user, or a --user override).
RUN git config --system safe.directory '*'

# Create a non-root user to run the server.  Using a fixed UID/GID
# avoids running as root, which limits blast radius if a vulnerability
# is exploited inside the container.
RUN groupadd -g 1000 mcp && useradd -u 1000 -g mcp -m mcp

# Install the CodeScene CLI (cs-tool) with integrity verification
# as root (needs write access to /usr/local or default install dir),
# then make it accessible to the non-root user.
ARG CS_CLI_INSTALLER_SHA256="6a119bd0746de31740bb899fbcc16f44b31df2392740642d5a29616961501f06"
RUN set -eu; \
    curl --proto '=https' --tlsv1.2 -fsSL -o /tmp/install-cs-tool.sh \
         https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh && \
    echo "${CS_CLI_INSTALLER_SHA256}  /tmp/install-cs-tool.sh" | sha256sum -c - && \
    HOME=/home/mcp bash /tmp/install-cs-tool.sh -y && \
    rm /tmp/install-cs-tool.sh && \
    chown -R mcp:mcp /home/mcp/.local

# Pre-create the config directory with correct ownership so that
# Docker named volumes inherit mcp:mcp permissions when first mounted.
RUN mkdir -p /home/mcp/.config/codehealth-mcp && \
    chown -R mcp:mcp /home/mcp/.config

# Copy the binary from the builder stage
COPY --from=builder /build/target/release/cs-mcp /usr/local/bin/cs-mcp

# Switch to the non-root user for all subsequent operations.
USER mcp:mcp

# Ensure the CodeScene CLI is on PATH
ENV PATH="/home/mcp/.local/bin:${PATH}"

LABEL io.modelcontextprotocol.server.name="com.codescene/codescene-mcp-server"

ENTRYPOINT ["/usr/local/bin/cs-mcp"]
