# CodeScene MCP Server

The **CodeScene MCP Server** exposes CodeScene’s [Code Health](https://codescene.com/product/code-health) analysis as local AI-friendly tools.

This server is designed to run in your local environment and lets AI assistants (like GitHub Copilot, Cursor, Claude code, etc.) request meaningful Code Health insights directly from your codebase. 
The Code Health insights augment the AI prompts with rich content around code quality issues, maintainability problems, and technical debt in general.

> [!NOTE]
> The server is under development. Expect to get a proper packaged installation + more tools soon. Real soon.

## Quick set-up

<details>

**<summary>VS Code</summary>**

Add the following to either your global `mcp.json` or your local `.vscode/mcp.json` file:

```json
{
	"servers": {
		"codescene-mcp": {
			"type": "stdio",
			"command": "docker",
			"args": [
				"run",
				"-i",
				"--rm",
				"-e",
				"CS_ACCESS_TOKEN",
				"codescene/codescene-mcp"
			]
		}
	}
}
```

</details>

## Use Cases

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