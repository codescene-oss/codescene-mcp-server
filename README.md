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
claude mcp add codescene --env CS_ACCESS_TOKEN=<token> -- docker run -i --rm -e CS_ACCESS_TOKEN -e CS_MOUNT_PATH=<PATH_TO_CODE> --mount type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro codescene/codescene-mcp
```

To connect with CodeScene On-prem:

```sh
claude mcp add codescene --env CS_ACCESS_TOKEN=<token> --env CS_ONPREM_URL=<url> -- docker run -i --rm -e CS_ACCESS_TOKEN -e CS_ONPREM_URL -e CS_MOUNT_PATH=<PATH_TO_CODE> --mount type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro codescene/codescene-mcp
```

Make sure to replace the `<PATH_TO_CODE>` with the absolute path to the directory whose read-only access you want the CodeScene MCP server to have.

</details>

<details>

**<summary>Codex CLI</summary>**

Configure `~/.codex/config.toml` depending on whether or not you use Cloud or On-prem.

CodeScene Cloud:

```toml
[mcp_servers.codescene]
command = "docker"
args = ["run", "--rm", "-i", "-e", "CS_ACCESS_TOKEN", "-e", "CS_MOUNT_PATH=<PATH_TO_CODE>", "--mount", "type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro", "codescene/codescene-mcp"]
env = { "CS_ACCESS_TOKEN" = "<YOUR_ACCESS_TOKEN>" }
```

CodeScene On-prem:

```toml
[mcp_servers.codescene]
command = "docker"
args = ["run", "--rm", "-i", "-e", "CS_ACCESS_TOKEN", "-e", "CS_ONPREM_URL", "-e", "CS_MOUNT_PATH=<PATH_TO_CODE>", "--mount", "type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro", "codescene/codescene-mcp"]
env = { "CS_ACCESS_TOKEN" = "<YOUR_ACCESS_TOKEN>", "CS_ONPREM_URL" = "<URL>" }
```

Make sure to replace the `<PATH_TO_CODE>` with the absolute path to the directory whose read-only access you want the CodeScene MCP server to have.

</details>

<details>

**<summary>GitHub Copilot CLI</summary>**

After starting Copilot CLI, run the following command to add the CodeScene MCP server:

```sh
/mcp add
```

You will then need to provide information about the MCP server.

CodeScene Cloud:

- Server Name: `codescene`
- Server Type: `Local (Press 1)`
- Command: `docker`
- Arguments: `run, --rm, -i, -e, CS_ACCESS_TOKEN, -e, CS_MOUNT_PATH=<PATH_TO_CODE>, --mount, "type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro", codescene/codescene-mcp`

CodeScene On-prem:

- Server Name: `codescene`
- Server Type: `Local (Press 1)`
- Command: `docker`
- Arguments: `run, --rm, -i, -e, CS_ACCESS_TOKEN, -e, CS_ONPREM_URL, -e, CS_MOUNT_PATH=<PATH_TO_CODE>, --mount, "type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro", codescene/codescene-mcp`

Make sure to replace the `<PATH_TO_CODE>` with the absolute path to the directory whose read-only access you want the CodeScene MCP server to have.

</details>

<details>

**<summary>GitHub Copilot coding agent</summary>**

GitHub Copilot coding agent can leverage the CodeScene MCP server directly in your CI/CD.

To add the secrets to your Copilot environment, follow the Copilot [documentation](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/extend-coding-agent-with-mcp#setting-up-a-copilot-environment-for-copilot-coding-agent). Only secrets with names prefixed with `COPILOT_MCP_` will be available to your MCP configuration.

In your GitHub repository, navigate under Settings -> Code & automation -> Copilot -> Coding agent, and add the following configuration in the MCP configuration section.

CodeScene Cloud:

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
        "-e",
        "CS_MOUNT_PATH=$CS_MOUNT_PATH",
        "--mount",
				"type=bind,src=$CS_MOUNT_PATH,dst=/mount/,ro",
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

CodeScene On-prem:

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
        "-e",
        "CS_ONPREM_URL=$CS_ONPREM_URL",
        "-e",
        "CS_MOUNT_PATH=$CS_MOUNT_PATH",
        "--mount",
				"type=bind,src=$CS_MOUNT_PATH,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "COPILOT_MCP_CS_ACCESS_TOKEN",
        "CS_ONPREM_URL": "COPILOT_MCP_CS_ONPREM_URL",
        "CS_MOUNT_PATH": "COPILOT_MCP_CS_MOUNT_PATH"
      },
      "tools": ["*"]
    }
  }
}
```

</details>

<details>

**<summary>Kiro</summary>**

Create a `.kiro/settings/mcp.json` file in your workspace directory (or edit if it already exists), add the following configuration.

CodeScene Cloud:

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
        "-e",
        "CS_MOUNT_PATH=<PATH_TO_CODE>",
        "--mount",
				"type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "<YOUR_TOKEN>",
      },
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

CodeScene On-prem:

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
        "-e",
        "CS_ONPREM_URL",
        "-e",
        "CS_MOUNT_PATH=<PATH_TO_CODE>",
        "--mount",
				"type=bind,src=<PATH_TO_CODE>,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "<YOUR_TOKEN>",
        "CS_ONPREM_URL": "<URL>"
      },
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

Make sure to replace the `<PATH_TO_CODE>` with the absolute path to the directory whose read-only access you want the CodeScene MCP server to have.

</details>

<details>

**<summary>VS Code</summary>**

[![Install CodeScene MCP for Cloud](https://img.shields.io/badge/VS_Code-Install_CodeScene_MCP_for_Cloud-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white)](https://insiders.vscode.dev/redirect/mcp/install?name=codescene&inputs=[%7B%22id%22%3A%22CS_MOUNT_PATH%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22Path%20of%20the%20directory%20that%20CodeScene%20should%20be%20able%20to%20see.%22%2C%22password%22%3Afalse%7D%2C%7B%22id%22%3A%22CS_ACCESS_TOKEN%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22CodeScene%20Access%20Token%22%2C%22password%22%3Atrue%7D]&config={%22command%22%3A%22docker%22%2C%22args%22%3A[%22run%22%2C%22-i%22%2C%22--rm%22%2C%22-e%22%2C%22CS_ACCESS_TOKEN%22%2C%22-e%22%2C%22CS_MOUNT_PATH%3D%24%7Binput%3ACS_MOUNT_PATH%7D%22%2C%22--mount%22%2C%22type%3Dbind%2Csrc%3D%24%7Binput%3ACS_MOUNT_PATH%7D%2Cdst%3D/mount/%2Cro%22%2C%22codescene/codescene-mcp%22]%2C%22env%22%3A%7B%22CS_ACCESS_TOKEN%22%3A%22%24%7Binput%3ACS_ACCESS_TOKEN%7D%22%7D%2C%22type%22%3A%22stdio%22})

[![Install CodeScene MCP for On-prem](https://img.shields.io/badge/VS_Code-Install_CodeScene_MCP_for_Onprem-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white)](https://insiders.vscode.dev/redirect/mcp/install?name=codescene&inputs=[%7B%22id%22%3A%22CS_MOUNT_PATH%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22Path%20of%20the%20directory%20that%20CodeScene%20should%20be%20able%20to%20see.%22%2C%22password%22%3Afalse%7D%2C%7B%22id%22%3A%22CS_ACCESS_TOKEN%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22CodeScene%20Access%20Token%22%2C%22password%22%3Atrue%7D%2C%7B%22id%22%3A%22CS_ONPREM_URL%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22CodeScene%20On-prem%20URL%22%2C%22password%22%3Afalse%7D]&config={%22command%22%3A%22docker%22%2C%22args%22%3A[%22run%22%2C%22-i%22%2C%22--rm%22%2C%22-e%22%2C%22CS_ACCESS_TOKEN%22%2C%22-e%22%2C%22CS_ONPREM_URL%22%2C%22-e%22%2C%22CS_MOUNT_PATH%3D%24%7Binput%3ACS_MOUNT_PATH%7D%22%2C%22--mount%22%2C%22type%3Dbind%2Csrc%3D%24%7Binput%3ACS_MOUNT_PATH%7D%2Cdst%3D/mount/%2Cro%22%2C%22codescene/codescene-mcp%22]%2C%22env%22%3A%7B%22CS_ACCESS_TOKEN%22%3A%22%24%7Binput%3ACS_ACCESS_TOKEN%7D%22%2C%22CS_ONPREM_URL%22%3A%22%24%7Binput%3ACS_ONPREM_URL%7D%22%7D%2C%22type%22%3A%22stdio%22})

</details>

### Get a `CS_ACCESS_TOKEN` for the MCP Server

The MCP server configuration requires a `CS_ACCESS_TOKEN` which you get via your CodeScene instance. (The token grants access to the code health analysis capability).
* For CodeScene Cloud you create the token [here](https://codescene.io/users/me/pat).
* In CodeScene on-prem, you get the token via `https://<your-cs-host><:port>/configuration/user/token`.

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

**Note:** if you want to use CodeScene On-prem, you need to additionally pass the `CS_ONPREM_URL` environment variable to it.

## Frequently Asked Questions

<details>

**<summary>I have multiple repos — how do I configure the MCP?</summary>**

Since you have to provide a mount path for Docker, you can either have a MCP configuration per project (in VS Code that would be a `.vscode/mcp.json` file per project, for example) or you can mount a root directory within which all your projects are and then just use that one configuration instead.

<details>

**<summary>Why are we mounting a directory in the Docker?</summary>**

Previously we had the MCP client pass the entire file contents to us in a JSON object, but with this we ran into a problem where if the file contents exceed your AI model's input or output token limit, we'd either get no data or incorrect data. 

While this might work for small files and code snippets, we want to provide a solution that works on any file, no matter the size, and we achieve this by having the MCP client return a file path to us which we then read ourselves, thus bypassing the AI token limit issue entirely.

To make this safe, we have you, the user, specify which path our MCP server should have access to. In addition, all the configuration examples provided in this README feature a mounting command that gives only read-only access to the mounted path, so we can't do anything to those files other than read them.

In addition this now saves your AI budget by not spending precious tokens on file reading, which can add up pretty quickly.

</details>

<details>

**<summary>Why do we specify `CS_MOUNT_PATH` twice?</summary>**

Due to the limitation of not knowing the relative path to the file from within Docker, in order to read the correct file we need to know the full absolute path to your mounted directory, so that we could deduce a relative path to the internally mounted file by simply taking the absolute path to the file, the absolute path to the mounted directory, and replacing the mounted directory part with our internal mounted directory. 

We pass the absolute path to the mounted directory to us via a environment variable `-e CS_MOUNT_PATH=<PATH>` so that we would know the absolute path, and then we need to pass that path again the second time via `--mount type=bind,src=<PATH>,dst=/mount/,ro` which then instructs Docker to actually mount `<PATH>` to our internal `/mount/` directory.

</details>