# CodeScene MCP Server

[![CodeScene Hotspot Code Health](https://codescene.io/projects/72556/status-badges/hotspot-code-health)](https://codescene.io/projects/72556)
[![CodeScene Average Code Health](https://codescene.io/projects/72556/status-badges/average-code-health)](https://codescene.io/projects/72556)
[![CodeScene System Mastery](https://codescene.io/projects/72556/status-badges/system-mastery)](https://codescene.io/projects/72556)

The **CodeScene MCP Server** exposes CodeScene’s [Code Health](https://codescene.com/product/code-health) analysis as local AI-friendly tools.

This server is designed to run in your local environment and lets AI assistants (like GitHub Copilot, Cursor, Claude code, etc.) request meaningful Code Health insights directly from your codebase. 
The Code Health insights augment the AI prompts with rich content around code quality issues, maintainability problems, and technical debt in general.

> [!NOTE]
> The server is under development. Expect to get a proper packaged installation + more tools soon. Real soon.

## Quick set-up

<details>

**<summary>Claude Code</summary>**

To connect with CodeScene Cloud:

```sh
claude mcp add codescene --env CS_ACCESS_TOKEN=<token> -- docker run -i --rm -e CS_ACCESS_TOKEN codescene/codescene-mcp
```

</details>

<details>

**<summary>Codex CLI</summary>**

To connect with CodeScene Cloud, add the following configuration to `~/.codex/config.toml`:

```toml
[mcp_servers.codescene]
command = "docker"
args = ["run", "--rm", "-i", "-e", "CS_ACCESS_TOKEN", "codescene/codescene-mcp"]
env = { "CS_ACCESS_TOKEN" = "<YOUR_ACCESS_TOKEN>" }
```

</details>

<details>

**<summary>GitHub Copilot CLI</summary>**

After starting Copilot CLI, run the following command to add the CodeScene MCP server:

```sh
/mcp add
```

You will then need to provide information about the MCP server:

- Server Name: `codescene`
- Server Type: `Local (Press 1)`
- Command: `docker`
- Arguments: `run, --rm, -i, -e, CS_ACCESS_TOKEN, codescene/codescene-mcp`

</details>

<details>

**<summary>GitHub Copilot coding agent</summary>**

GitHub Copilot coding agent can leverage the CodeScene MCP server directly in your CI/CD.

To add the secrets to your Copilot environment, follow the Copilot [documentation](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/extend-coding-agent-with-mcp#setting-up-a-copilot-environment-for-copilot-coding-agent). Only secrets with names prefixed with `COPILOT_MCP_` will be available to your MCP configuration.

In your GitHub repository, navigate under Settings -> Code & automation -> Copilot -> Coding agent, and add the following configuration in the MCP configuration section:

```json
{
  "mcpServers": {
    "codescene": {
      "type": "local",
      "command": "docker",
      "args": [
        "run",
        "--rm",
        "-i",
        "-e",
        "CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "COPILOT_MCP_CS_ACCESS_TOKEN"
      },
      "tools": ["*"]
    }
  }
}
```

</details>

<details>

**<summary>Kiro</summary>**

Create a `.kiro/settings/mcp.json` file in your workspace directory (or edit if it already exists), add the following configuration:

```json
{
  "mcpServers": {
    "sonarqube": {
      "command": "docker",
      "args": [
        "run",
        "-i",
        "--rm",
        "-e", 
        "CS_ACCESS_TOKEN",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEEN": "<YOUR_TOKEN>",
      },
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

</details>

<details>

**<summary>VS Code</summary>**

[![Install for CodeScene Cloud](https://img.shields.io/badge/VS_Code-Install_for_CodeScene_Cloud-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white)](https://insiders.vscode.dev/redirect/mcp/install?name=codescene&inputs=[{%22id%22:%22CS_ACCESS_TOKEN%22,%22type%22:%22promptString%22,%22description%22:%22CodeScene%20Access%20Token%22,%22password%22:true}]&config={%22command%22:%22docker%22,%22args%22:[%22run%22,%22-i%22,%22--rm%22,%22-e%22,%22CS_ACCESS_TOKEN%22,%22codescene/codescene-mcp%22],%22env%22:{%22CS_ACCESS_TOKEN%22:%22${input:CS_ACCESS_TOKEN}%22}})

</details>

## Use Cases

> [!TIP]
> Watch the [demo video of the CodeScene MCP](https://www.youtube.com/watch?v=AycLVxKmVSY).

With the CodeScene MCP Server in place, your AI tools can:

### Safeguard AI-Generated Code
Prevent AI from introducing technical debt by flagging maintainability issues like complexity, deep nesting, low cohesion, etc.

### Make Targeted Refactoring  
AI tools can refactor code, but they lack direction on *what* to fix and *how to measure* if it helped.  
The Code Health tools solve this by giving AI assistants precise insight into design problems, as well as an objective way to assess the outcome: **did the Code Health improve?**

### Understand Existing Code Before Acting
Use Code Health reviews to inform AI-driven summaries, diagnostics, or code transformations based on **real-world cognitive and design challenges**, not just syntax.

## Building the docker instance locally

You can build and run the dockerized CodeScene MCP server by first cloning the repo and then building the Docker image:

```sh
docker build -t codescene-mcp .
```

And then configuring the MCP in your editor, for example in VS Code:

```json
"codescene-mcp": {
    "type": "stdio",
    "command": "docker",
    "args": [
        "run",
        "-i",
        "--rm",
        "-e",
        "CS_ACCESS_TOKEN",
        "codescene-mcp"
    ]
}
```

This configuration will automatically pick up the `CS_ACCESS_TOKEN` environment variable, but if you can't create a system-wide one then you can manually specify it like this:

```json
"codescene-mcp": {
    "type": "stdio",
    "command": "docker",
    "args": [
        "run",
        "-i",
        "--rm",
        "-e",
        "CS_ACCESS_TOKEN",
        "codescene-mcp"
    ],
    "env": {
		"CS_ACCESS_TOKEN": "token-goes-here",
    }
}
```

Or when running it from the CLI, like this:

```sh
docker run -i --rm -e CS_ACCESS_TOKEN=token-goes-here codescene-mcp
```
