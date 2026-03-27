# Building a Static Linux Executable

Rust can produce fully static Linux binaries using the `musl` target. This avoids any glibc version dependencies.

## Building locally (with musl)

1. Add the musl target:

```sh
rustup target add x86_64-unknown-linux-musl
```

2. Install musl tools (Ubuntu/Debian):

```sh
sudo apt-get install musl-tools
```

3. Build:

```sh
cargo build --release --target x86_64-unknown-linux-musl
```

The binary is produced at `target/x86_64-unknown-linux-musl/release/cs-mcp`.

## Building via Docker

If you don't have a Linux environment or musl toolchain set up, you can use Docker to build:

```sh
docker run --rm -v "$(pwd)":/build -w /build rust:1-bookworm bash -c \
  "apt-get update && apt-get install -y musl-tools && \
   rustup target add x86_64-unknown-linux-musl && \
   cargo build --release --target x86_64-unknown-linux-musl"
```

The binary will be at `target/x86_64-unknown-linux-musl/release/cs-mcp`.

## ARM64 (aarch64)

For aarch64 static builds, use the `aarch64-unknown-linux-musl` target instead:

```sh
rustup target add aarch64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

## CI

The GitHub Actions CI workflow handles Linux builds automatically using the same musl target approach.
