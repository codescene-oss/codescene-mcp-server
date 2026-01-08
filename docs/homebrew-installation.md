# Installing CodeScene MCP Server via Homebrew

You can install the CodeScene MCP Server using Homebrew on macOS and Linux.

## Quick Installation

```bash
# Add the CodeScene tap
brew tap codescene-oss/codescene-mcp-server https://github.com/codescene-oss/codescene-mcp-server

# Install CodeScene MCP Server
brew install cs-mcp
```

## Usage

After installation, the `cs-mcp` command will be available in your PATH:

```bash
cs-mcp
```

## Updating

To update to the latest version:

```bash
brew update
brew upgrade cs-mcp
```

## Uninstalling

```bash
brew uninstall cs-mcp
brew untap codescene-oss/codescene-mcp-server
```

## Supported Platforms

| Platform       | Architecture |
|----------------|--------------|
| macOS          | ARM64 (Apple Silicon) |
| macOS          | AMD64 (Intel) |
| Linux          | ARM64 |
| Linux          | AMD64 |

## Using with AI Assistants

After installing via Homebrew, you can configure your AI assistant to use the binary directly instead of Docker.

### Example: Claude Code

```bash
export CS_ACCESS_TOKEN="<your token here>"
claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN -- cs-mcp
```

### Example: VS Code / GitHub Copilot

In your VS Code settings (`settings.json`):

```json
{
  "mcp": {
    "servers": {
      "codescene": {
        "command": "cs-mcp",
        "env": {
          "CS_ACCESS_TOKEN": "<your token here>"
        }
      }
    }
  }
}
```

## Troubleshooting

### Binary not found after installation

Make sure Homebrew's bin directory is in your PATH:

```bash
# For macOS (Apple Silicon)
export PATH="/opt/homebrew/bin:$PATH"

# For macOS (Intel) or Linux
export PATH="/usr/local/bin:$PATH"
```
