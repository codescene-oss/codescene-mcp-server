# CodeScene MCP Server

[![CodeScene Hotspot Code Health](https://codescene.io/projects/72556/status-badges/hotspot-code-health)](https://codescene.io/projects/72556)
[![CodeScene Average Code Health](https://codescene.io/projects/72556/status-badges/average-code-health)](https://codescene.io/projects/72556)
[![CodeScene System Mastery](https://codescene.io/projects/72556/status-badges/system-mastery)](https://codescene.io/projects/72556)

The **CodeScene MCP Server** exposes CodeScene's [Code Health](https://codescene.com/product/code-health) analysis as local AI-friendly tools.

This server is designed to run in your local environment and lets AI assistants (like GitHub Copilot, Cursor, Claude code, etc.) request meaningful Code Health insights directly from your codebase. 
The Code Health insights augment the AI prompts with rich content around code quality issues, maintainability problems, and technical debt in general.

## Getting Started with CodeScene MCP

1. Get an Access Token for the MCP Server — see [Getting a Personal Access Token](docs/getting-a-personal-access-token.md).
2. Install the MCP Server using one of the [installation options](#installation) below.
3. Add the MCP Server to your AI assistant. See the detailed instructions for your environment in the installation guide.
4. Copy the file [AGENTS.md](AGENTS.md) to your repository. This file guides AI agents on how to use the MCP, e.g. rules to safeguard AI coding.
   * If you use Amazon Q, then you want to copy our [.amazonq/rules](.amazonq/rules) to your repository instead.

## Installation

Choose the installation method that works best for your platform.

<details>
<summary><b>NPM / npx (macOS, Linux, Windows)</b></summary>

Run the MCP server directly with npx (no install needed):

```bash
npx @codescene/codehealth-mcp
```

Or install globally:

```bash
npm install -g @codescene/codehealth-mcp
```

The first run automatically downloads the correct platform-specific binary for your system and caches it for future use. Requires [Node.js](https://nodejs.org/) 18 or later.

📖 **[Full installation & integration guide](docs/npm-installation.md)**

</details>

<details>
<summary><b>Homebrew (macOS / Linux)</b></summary>

```bash
brew tap codescene-oss/codescene-mcp-server https://github.com/codescene-oss/codescene-mcp-server
brew install cs-mcp
```

📖 **[Full installation & integration guide](docs/homebrew-installation.md)**

</details>

<details>
<summary><b>Windows</b></summary>

Run this in PowerShell:

```powershell
irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/install.ps1 | iex
```

📖 **[Full installation & integration guide](docs/windows-installation.md)**

</details>

<details>
<summary><b>Manual Download</b></summary>

Download the latest binary for your platform from the [GitHub Releases page](https://github.com/codescene-oss/codescene-mcp-server/releases):

- **macOS:** `cs-mcp-macos-aarch64.zip` (Apple Silicon) or `cs-mcp-macos-amd64` (Intel)
- **Linux:** `cs-mcp-linux-aarch64.zip` or `cs-mcp-linux-amd64`
- **Windows:** `cs-mcp-windows-amd64.exe`

After downloading, make it executable and optionally add it to your PATH:

```bash
chmod +x cs-mcp-*
mv cs-mcp-* /usr/local/bin/cs-mcp
```

You can also [build a static executable from source](docs/building-executable-locally.md).

</details>

<details>
<summary><b>Docker</b></summary>

```bash
docker pull codescene/codescene-mcp
```

📖 **[Full installation & integration guide](docs/docker-installation.md)** | [Build the Docker image locally](docs/building-docker-locally.md)

</details>

---

## Use Cases

> [!TIP]
> Watch the [demo video of the CodeScene MCP](https://www.youtube.com/watch?v=AycLVxKmVSY).

> [!NOTE]
> CodeScene MCP comes with a set of [example prompts](.github/prompts) and an [AGENTS.md](AGENTS.md) file to capture the key use cases and guide your AI agents. Copy the `AGENTS.md` file to your own repository.

With the CodeScene MCP Server in place, your AI tools can:

### Safeguard AI-Generated Code
Prevent AI from introducing technical debt by flagging maintainability issues like complexity, deep nesting, low cohesion, etc.

### Uplifting Unhealthy Code for AI Readiness: Refactoring With ACE + AI
AI works best on healthy, modular code. Many legacy functions are too large or complex for reliable AI refactoring, which leads to poor suggestions and unstable changes.  
[CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring), exposed through the MCP server, helps by *first* restructuring these complex functions into smaller and more cohesive units. This modularity makes the code far easier for AI agents to understand and refactor safely.

The result is a cooperative workflow where:  
- **CodeScene ACE improves modularity and structure**,  
- **AI performs more precise refactorings**, and  
- **Code Health guides both toward maintainable outcomes**.

🎗️ ACE is a **CodeScene add-on** and requires an additional license. You can [request access and more info here](https://codescene.com/contact-us-about-codescene-ace).

#### Activate ACE in CodeScene MCP

To enable ACE, add one extra environment variable: `CS_ACE_ACCESS_TOKEN`, which you receive when you purchase the ACE add-on.
The exact setup depends on your editor or AI assistant, but you simply need to pass this token into the MCP server.

Here's an example for VS Code, where the variable appears in both `args` and `env`:
```json
"codescene": {
  "command": "docker",
  "args": [
    "run",
    "-i",
    "--rm",
    "-e", "CS_ACCESS_TOKEN",
    "-e", "CS_ONPREM_URL",
    "-e", "CS_ACE_ACCESS_TOKEN",
    "-e", "CS_MOUNT_PATH=${input:CS_MOUNT_PATH}",
    "--mount",
    "type=bind,src=${input:CS_MOUNT_PATH},dst=/mount/,ro",
    "codescene-mcp"
  ],
  "env": {
    "CS_ACCESS_TOKEN":     "${input:CS_ACCESS_TOKEN}",
    "CS_ONPREM_URL":       "${input:CS_ONPREM_URL}",
    "CS_ACE_ACCESS_TOKEN": "${input:CS_ACE_ACCESS_TOKEN}"
  },
  "type": "stdio"
}
```
Use the same principle for any other environment: just make sure `CS_ACE_ACCESS_TOKEN` is passed to the MCP server.

### Make Targeted Refactoring  
AI tools can refactor code, but they lack direction on *what* to fix and *how to measure* if it helped.  
The Code Health tools solve this by giving AI assistants precise insight into design problems, as well as an objective way to assess the outcome: **did the Code Health improve?**

### Understand Existing Code Before Acting
Use Code Health reviews to inform AI-driven summaries, diagnostics, or code transformations based on **real-world cognitive and design challenges**, not just syntax.

## Frequently Asked Questions

<details>

<summary>Do I need a CodeScene account to use the MCP?</summary>

The full feature set — including hotspots, technical debt goals, and code ownership — requires a [CodeScene subscription](https://codescene.com/pricing). Use your CodeScene instance to create the `CS_ACCESS_TOKEN` which activates the MCP.
The MCP supports both CodeScene Cloud and CodeScene on-prem.

For local Code Health analysis without a CodeScene subscription, you can use the standalone [CodeScene Code Health MCP](https://codescene.com/product/codehealth-mcp-server).

</details>

<details>

<summary>How does the MCP Server keep my code private and secure?</summary>

The CodeScene MCP Server runs fully locally. All analysis — including Code Health scoring, delta reviews, and business-case calculations — is performed on your machine, against your local repository.
No source code or analysis data is sent to cloud providers, LLM vendors, or any external service.

Analysis results (e.g. hotspots and technical debt goals) are fetched via REST from your own CodeScene account using a secure token.

For complete details, please see CodeScene's full [privacy and security documentation](https://codescene.com/policies).

</details>

<details>

<summary>Can I use any LLM as the backbone for CodeScene MCP?</summary>

CodeScene MCP can work with any model your AI assistant supports, but we strongly recommend choosing a frontier model when your assistant offers a model selector (as in tools like GitHub Copilot). 

Frontier models -- such as Claude Sonnet -- deliver far better rule adherence and refactoring quality, while legacy models like GPT-4.1 often struggle with MCP constraints. 
For a consistent, high-quality experience, select the newest available model.

</details>

<details>

<summary>I have multiple repos — how do I configure the MCP?</summary>

Since you have to provide a mount path for Docker, you can either have a MCP configuration per project (in VS Code that would be a `.vscode/mcp.json` file per project, for example) or you can mount a root directory within which all your projects are and then just use that one configuration instead.

</details>

<details>

<summary>Why does IntelliJ give a wrong path to the MCP server?</summary>

In our testing we've seen that IntelliJ's AI Assistant sometimes gives a wrong path to the CodeScene MCP server. 
From what we can tell, it seems to have nothing to do with the MCP server itself, but rather with IntelliJ's AI Assistant, which 
seems to hallucinate parts of the path some of the time. We're still investigating this issue and will update this section once we have more information.

</details>

<details>

<summary>How do I configure custom SSL certificates?</summary>

If your organization uses an internal CA (Certificate Authority), you need to configure the MCP server to trust that certificate.

Set the `REQUESTS_CA_BUNDLE` environment variable to point to your CA certificate file (PEM format). This single variable configures SSL for both the Python MCP server and the embedded Java-based CodeScene CLI—the MCP server automatically converts the PEM certificate to a Java-compatible truststore at runtime.

**For the static binary (Homebrew/Windows/npx):**
```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://your-codescene-instance.example.com",
        "REQUESTS_CA_BUNDLE": "/path/to/your/internal-ca.crt"
      }
    }
  }
}
```

**For Docker:**
Mount your certificate into the container and set `REQUESTS_CA_BUNDLE` to the path *inside* the container:
```json
{
  "command": "docker",
  "args": [
    "run", "-i", "--rm",
    "-e", "CS_ACCESS_TOKEN",
    "-e", "CS_ONPREM_URL",
    "-e", "REQUESTS_CA_BUNDLE=/certs/internal-ca.crt",
    "-e", "CS_MOUNT_PATH=${input:CS_MOUNT_PATH}",
    "--mount", "type=bind,src=${input:CS_MOUNT_PATH},dst=/mount/,ro",
    "--mount", "type=bind,src=/path/to/your/certs/internal-ca.crt,dst=/certs/internal-ca.crt,ro",
    "codescene/codescene-mcp"
  ],
  "env": {
    "CS_ACCESS_TOKEN": "${input:CS_ACCESS_TOKEN}",
    "CS_ONPREM_URL": "${input:CS_ONPREM_URL}"
  }
}
```

> **Note:** The `REQUESTS_CA_BUNDLE` value (`/certs/internal-ca.crt`) must match the destination path in the mount (`dst=/certs/internal-ca.crt`).

The MCP also supports `SSL_CERT_FILE` and `CURL_CA_BUNDLE` as alternatives to `REQUESTS_CA_BUNDLE`.

For detailed configuration examples, see:
- [NPM/npx SSL configuration](docs/npm-installation.md#custom-ssltls-certificates)
- [Docker SSL configuration](docs/docker-installation.md#custom-ssltls-certificates)
- [Homebrew/static binary SSL configuration](docs/homebrew-installation.md#custom-ssltls-certificates)
- [Windows SSL configuration](docs/windows-installation.md#custom-ssltls-certificates)

</details>

<details>

<summary>How do I disable the version update check?</summary>

The MCP server periodically checks GitHub for newer releases and shows a "VERSION UPDATE AVAILABLE" banner when your version is outdated. This check runs in the background and never blocks tool responses, but in network-restricted environments you may want to disable it entirely.

Set the `CS_DISABLE_VERSION_CHECK` environment variable to any non-empty value (e.g. `1`):

**For the static binary (Homebrew/Windows/npx):**
```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_DISABLE_VERSION_CHECK": "1"
      }
    }
  }
}
```

**For Docker:**
```json
{
  "command": "docker",
  "args": [
    "run", "-i", "--rm",
    "-e", "CS_ACCESS_TOKEN",
    "-e", "CS_DISABLE_VERSION_CHECK=1",
    "-e", "CS_MOUNT_PATH=${input:CS_MOUNT_PATH}",
    "--mount", "type=bind,src=${input:CS_MOUNT_PATH},dst=/mount/,ro",
    "codescene/codescene-mcp"
  ],
  "env": {
    "CS_ACCESS_TOKEN": "${input:CS_ACCESS_TOKEN}"
  }
}
```

When disabled, no network traffic is made to the version check endpoint and no version banner will appear.

</details>
