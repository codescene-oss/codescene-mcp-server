# Installing CodeScene MCP Server via Docker

Docker provides a self-contained way to run the CodeScene MCP Server without installing any dependencies. This is the recommended method for most users.

## Prerequisites

- [Docker](https://docs.docker.com/get-started/get-docker/) installed and running
- A CodeScene account with an access token (see [Getting a Personal Access Token](getting-a-personal-access-token.md))

## Quick Start

Pull the latest image:

```bash
docker pull codescene/codescene-mcp
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `CS_ACCESS_TOKEN` | Yes | Your CodeScene personal access token |
| `CS_MOUNT_PATH` | Yes | Absolute path to your code directory |
| `CS_ONPREM_URL` | Only for on-prem | URL to your CodeScene on-prem instance |
| `CS_ACE_ACCESS_TOKEN` | Optional | Token for CodeScene ACE refactoring (requires license) |
| `HTTPS_PROXY` | Optional | HTTPS proxy URL (e.g., `http://proxy.example.com:8080`) |
| `HTTP_PROXY` | Optional | HTTP proxy URL |
| `NO_PROXY` | Optional | Comma-separated list of hosts to bypass proxy |

## Integration with AI Assistants

### Claude Code

**CodeScene Cloud:**

```sh
export CS_ACCESS_TOKEN="<your token here>"
export PATH_TO_CODE="<your project dir here>"

claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN -- docker run -i --rm -e CS_ACCESS_TOKEN -e CS_MOUNT_PATH=$PATH_TO_CODE --mount type=bind,src=$PATH_TO_CODE,dst=/mount/,ro codescene/codescene-mcp
```

**CodeScene On-prem:**

```sh
export CS_ACCESS_TOKEN="<your token here>"
export CS_ONPREM_URL="<your onprem url>"
export PATH_TO_CODE="<your project dir here>"

claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN --env CS_ONPREM_URL=$CS_ONPREM_URL -- docker run -i --rm -e CS_ACCESS_TOKEN -e CS_ONPREM_URL -e CS_MOUNT_PATH=$PATH_TO_CODE --mount type=bind,src=$PATH_TO_CODE,dst=/mount/,ro codescene/codescene-mcp
```

### Codex CLI

Configure `~/.codex/config.toml` (replace `/path/to/your/code` with your actual code directory path):

**CodeScene Cloud:**

```toml
[mcp_servers.codescene]
command = "docker"
args = ["run", "--rm", "-i", "-e", "CS_ACCESS_TOKEN", "-e", "CS_MOUNT_PATH=/path/to/your/code", "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro", "codescene/codescene-mcp"]
env = { "CS_ACCESS_TOKEN" = "your-token-here" }
```

**CodeScene On-prem:**

```toml
[mcp_servers.codescene]
command = "docker"
args = ["run", "--rm", "-i", "-e", "CS_ACCESS_TOKEN", "-e", "CS_ONPREM_URL", "-e", "CS_MOUNT_PATH=/path/to/your/code", "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro", "codescene/codescene-mcp"]
env = { "CS_ACCESS_TOKEN" = "your-token-here", "CS_ONPREM_URL" = "https://your-codescene-instance.example.com" }
```

### GitHub Copilot CLI

After starting Copilot CLI, run `/mcp add` and provide (replace `/path/to/your/code` with your actual code directory path):

- Server Name: `codescene`
- Server Type: `Local (Press 1)`
- Command: `docker`
- Arguments (Cloud): `run, --rm, -i, -e, CS_ACCESS_TOKEN, -e, CS_MOUNT_PATH=/path/to/your/code, --mount, "type=bind,src=/path/to/your/code,dst=/mount/,ro", codescene/codescene-mcp`
- Arguments (On-prem): `run, --rm, -i, -e, CS_ACCESS_TOKEN, -e, CS_ONPREM_URL, -e, CS_MOUNT_PATH=/path/to/your/code, --mount, "type=bind,src=/path/to/your/code,dst=/mount/,ro", codescene/codescene-mcp`

### GitHub Copilot Coding Agent

Add to your repository's Copilot MCP configuration (Settings → Code & automation → Copilot → Coding agent):

**CodeScene Cloud:**

```json
{
  "mcpServers": {
    "codescene": {
      "type": "local",
      "command": "docker",
      "args": [
        "run", "--rm", "-i",
        "-e", "CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN",
        "-e", "CS_MOUNT_PATH=$CS_MOUNT_PATH",
        "--mount", "type=bind,src=$CS_MOUNT_PATH,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "COPILOT_MCP_CS_ACCESS_TOKEN",
        "CS_MOUNT_PATH": "COPILOT_MCP_CS_MOUNT_PATH"
      },
      "tools": ["*"]
    }
  }
}
```

### VS Code / GitHub Copilot

Use the one-click install buttons or add to your `settings.json` or `.vscode/mcp.json`:

[![Install CodeScene MCP for Cloud](https://img.shields.io/badge/VS_Code-Install_CodeScene_MCP_for_Cloud-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white)](https://insiders.vscode.dev/redirect/mcp/install?name=codescene&inputs=[%7B%22id%22%3A%22CS_MOUNT_PATH%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22Path%20of%20the%20directory%20that%20CodeScene%20should%20be%20able%20to%20see.%22%2C%22password%22%3Afalse%7D%2C%7B%22id%22%3A%22CS_ACCESS_TOKEN%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22CodeScene%20Access%20Token%22%2C%22password%22%3Atrue%7D]&config={%22command%22%3A%22docker%22%2C%22args%22%3A[%22run%22%2C%22-i%22%2C%22--rm%22%2C%22-e%22%2C%22CS_ACCESS_TOKEN%22%2C%22-e%22%2C%22CS_MOUNT_PATH%3D%24%7Binput%3ACS_MOUNT_PATH%7D%22%2C%22--mount%22%2C%22type%3Dbind%2Csrc%3D%24%7Binput%3ACS_MOUNT_PATH%7D%2Cdst%3D/mount/%2Cro%22%2C%22codescene/codescene-mcp%22]%2C%22env%22%3A%7B%22CS_ACCESS_TOKEN%22%3A%22%24%7Binput%3ACS_ACCESS_TOKEN%7D%22%7D%2C%22type%22%3A%22stdio%22})

[![Install CodeScene MCP for On-prem](https://img.shields.io/badge/VS_Code-Install_CodeScene_MCP_for_Onprem-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white)](https://insiders.vscode.dev/redirect/mcp/install?name=codescene&inputs=[%7B%22id%22%3A%22CS_MOUNT_PATH%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22Path%20of%20the%20directory%20that%20CodeScene%20should%20be%20able%20to%20see.%22%2C%22password%22%3Afalse%7D%2C%7B%22id%22%3A%22CS_ACCESS_TOKEN%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22CodeScene%20Access%20Token%22%2C%22password%22%3Atrue%7D%2C%7B%22id%22%3A%22CS_ONPREM_URL%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22CodeScene%20On-prem%20URL%22%2C%22password%22%3Afalse%7D]&config={%22command%22%3A%22docker%22%2C%22args%22%3A[%22run%22%2C%22-i%22%2C%22--rm%22%2C%22-e%22%2C%22CS_ACCESS_TOKEN%22%2C%22-e%22%2C%22CS_ONPREM_URL%22%2C%22-e%22%2C%22CS_MOUNT_PATH%3D%24%7Binput%3ACS_MOUNT_PATH%7D%22%2C%22--mount%22%2C%22type%3Dbind%2Csrc%3D%24%7Binput%3ACS_MOUNT_PATH%7D%2Cdst%3D/mount/%2Cro%22%2C%22codescene/codescene-mcp%22]%2C%22env%22%3A%7B%22CS_ACCESS_TOKEN%22%3A%22%24%7Binput%3ACS_ACCESS_TOKEN%7D%22%2C%22CS_ONPREM_URL%22%3A%22%24%7Binput%3ACS_ONPREM_URL%7D%22%7D%2C%22type%22%3A%22stdio%22})

Manual configuration (replace `/path/to/your/code` with your actual code directory path):

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

### Kiro

Create a `.kiro/settings/mcp.json` file in your workspace (replace `/path/to/your/code` with your actual code directory path):

```json
{
  "mcpServers": {
    "codescene": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      },
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

### Amazon Q CLI

Replace `/path/to/your/code` with your actual code directory path:

```sh
q mcp add --name codescene-mcp --command docker --args '["run", "--rm", "-i", "-e", "CS_ACCESS_TOKEN", "-e", "CS_MOUNT_PATH=/path/to/your/code", "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro", "codescene/codescene-mcp"]'
```

### Amazon Q IDE

1. Access the MCP configuration UI
2. Add a new server with name `CodeSceneMCPServer`
3. Select `stdio` as the transport protocol
4. Command: `docker`
5. Arguments: `run`, `--rm`, `-i`, `-e`, `CS_ACCESS_TOKEN`, `-e`, `CS_MOUNT_PATH=/path/to/your/code`, `--mount`, `type=bind,src=/path/to/your/code,dst=/mount/,ro`, `codescene/codescene-mcp`
6. Add environment variables for `CS_ACCESS_TOKEN` (and `CS_ONPREM_URL` if using on-prem)

### Claude Desktop

Claude Desktop is available for macOS and Windows. Add to your configuration file:
- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

**CodeScene Cloud:**

```json
{
  "mcpServers": {
    "codescene": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

**CodeScene On-prem:**

```json
{
  "mcpServers": {
    "codescene": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_ONPREM_URL",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://your-codescene-instance.example.com"
      }
    }
  }
}
```

> **Note:** Replace `/path/to/your/code` with the actual absolute path to your code directory (e.g., `/Users/jane/projects/my-app` on macOS or `C:\Users\Jane\projects\my-app` on Windows). After saving the configuration, restart Claude Desktop.

## Enabling CodeScene ACE

[CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) provides AI-powered refactoring capabilities. To enable it, add the `CS_ACE_ACCESS_TOKEN` environment variable (replace `/path/to/your/code` with your actual code directory path):

```json
{
  "command": "docker",
  "args": [
    "run", "-i", "--rm",
    "-e", "CS_ACCESS_TOKEN",
    "-e", "CS_ACE_ACCESS_TOKEN",
    "-e", "CS_MOUNT_PATH=/path/to/your/code",
    "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
    "codescene/codescene-mcp"
  ],
  "env": {
    "CS_ACCESS_TOKEN": "your-token-here",
    "CS_ACE_ACCESS_TOKEN": "your-ace-token-here"
  }
}
```

## Building Docker Image Locally

See [Building the Docker image locally](building-docker-locally.md) for instructions on building the image from source.
