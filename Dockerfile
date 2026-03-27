# ── Stage 1: Build Rust binary ────────────────────────────────────────────────
FROM rust:1-bookworm AS builder

# Version injected at build time (e.g., "MCP-1.2.3"); passed to build.rs
# via the CS_MCP_VERSION env var to embed in the binary.
ARG VERSION=dev

WORKDIR /build

# Copy only what the build needs (keeps layer cache friendly)
COPY Cargo.toml Cargo.lock build.rs ./
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
# The container runs as root but the mounted directory is typically owned
# by the host user (e.g., UID 1001 on Linux CI), which triggers git's
# "dubious ownership" safe-directory check (git 2.35.2+).
RUN git config --global safe.directory '*'

# Install the CodeScene CLI (cs-tool)
RUN curl https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh | bash -s -- -y

# Copy the binary from the builder stage
COPY --from=builder /build/target/release/cs-mcp /usr/local/bin/cs-mcp

# Ensure the CodeScene CLI is on PATH
ENV PATH="/root/.local/bin:${PATH}"

LABEL io.modelcontextprotocol.server.name="com.codescene/codescene-mcp-server"

ENTRYPOINT ["/usr/local/bin/cs-mcp"]
