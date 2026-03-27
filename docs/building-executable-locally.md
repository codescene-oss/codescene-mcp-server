# Building a Static Executable Locally

You can compile the MCP server from source using Rust's Cargo build system.

## Prerequisites

- **Rust toolchain** (1.75 or later) — install via [rustup](https://rustup.rs/)

## Build Steps

1. Clone the repository:

```sh
git clone https://github.com/codescene-oss/codescene-mcp.git
cd codescene-mcp
```

2. Build the release binary:

```sh
cargo build --release
```

This produces a single `cs-mcp` binary at `target/release/cs-mcp` (or `target\release\cs-mcp.exe` on Windows).

## Using the Executable

Configure your MCP client to use the executable directly instead of Docker. For example, in VS Code (`.vscode/mcp.json`):

```json
{
  "servers": {
    "codescene-mcp": {
      "type": "stdio",
      "command": "/path/to/cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

Since the executable runs natively without Docker, you don't need to configure mount paths — the MCP server can access files directly from your filesystem.
