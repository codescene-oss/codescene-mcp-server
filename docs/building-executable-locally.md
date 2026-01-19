# Building a Static Executable Locally

If you prefer a standalone binary instead of Docker, you can compile the MCP server into a single executable using [Nuitka](https://nuitka.net/).

## Prerequisites

- **Python 3.13** (required)
- **Nuitka** (`pip install Nuitka`)
- **CodeScene CLI** binary (`cs`) for your platform, downloaded from:
  - Linux amd64: https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip
  - Linux aarch64: https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip
  - macOS amd64: https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip
  - macOS aarch64: https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip
  - Windows amd64: https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip

## Build Steps

1. Clone the repository and set up a virtual environment:

```sh
git clone https://github.com/codescene-oss/codescene-mcp.git
cd codescene-mcp
python3.13 -m venv .venv
source .venv/bin/activate  # On Windows: .venv\Scripts\activate
```

2. Install dependencies:

```sh
pip install -r src/requirements.txt
pip install Nuitka
```

3. Download and extract the CodeScene CLI for your platform. For example, on macOS aarch64:

```sh
wget https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip -O codescene-cli.zip
unzip codescene-cli.zip -d .
```

4. Build the executable:

**Linux / macOS:**
```sh
python3.13 -m nuitka --onefile \
  --assume-yes-for-downloads \
  --include-data-dir=./src/docs=src/docs \
  --include-data-files=./cs=cs \
  --output-filename=cs-mcp \
  src/cs_mcp_server.py
```

**Windows:**
```powershell
python -m nuitka --onefile `
  --assume-yes-for-downloads `
  --include-data-dir=./src/docs=src/docs `
  --include-data-files=./cs.exe=cs.exe `
  --output-filename=cs-mcp.exe `
  src/cs_mcp_server.py
```

This will produce a single `cs-mcp` (or `cs-mcp.exe` on Windows) executable that bundles the MCP server, its dependencies, and the CodeScene CLI.

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
