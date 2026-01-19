# Building the Docker Instance Locally

You can build and run the dockerized CodeScene MCP server by first cloning the repo and then building the Docker image:

```sh
docker build -t codescene-mcp .
```

And then configuring the MCP in your editor. For example, in VS Code (`.vscode/mcp.json`), replace `/path/to/your/code` with your actual code directory path:

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
        "-e",
        "CS_MOUNT_PATH=/path/to/your/code",
        "--mount",
        "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene-mcp"
      ]
    }
  }
}
```

This configuration will automatically pick up the `CS_ACCESS_TOKEN` environment variable, but if you can't create a system-wide one then you can manually specify it like this:

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
        "-e",
        "CS_MOUNT_PATH=/path/to/your/code",
        "--mount",
        "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

Or when running it from the CLI, like this:

```sh
docker run -i --rm -e CS_ACCESS_TOKEN=token-goes-here codescene-mcp
```

**Note:** if you want to use CodeScene On-prem, you need to additionally pass the `CS_ONPREM_URL` environment variable to it.
