# Building Linux Executables in Docker (glibc 2.28)

This project includes a local Docker-based build flow for Linux binaries.
It uses manylinux 2.28 so you can iterate locally and reuse the exact same
build command in CI.

## Prerequisites

- Docker
- A checked-out repository root

## Build locally

From the repository root:

```sh
./scripts/build-linux-exe.sh amd64
```

Or for ARM64:

```sh
./scripts/build-linux-exe.sh aarch64
```

This produces one of these files in the repository root:

- `cs-mcp-linux-amd64`
- `cs-mcp-linux-aarch64`

You can override the embedded version string:

```sh
VERSION=MCP-0.0.0-local ./scripts/build-linux-exe.sh amd64
```

## Files used

- Builder image definition: `docker/linux-exe-builder.Dockerfile`
- Host entry script: `scripts/build-linux-exe.sh`
- In-container build script: `scripts/build-linux-exe-inner.sh`

The GitHub workflow uses the same host script for Linux builds.

## GLIBC guard

CI validates Linux binaries with `scripts/check_glibc_max.py` and fails if
any inspected binary requires a GLIBC version above `2.28`.
