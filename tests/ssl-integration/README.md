# SSL Integration Tests

End-to-end integration tests that verify the **MCP SERVER** (which embeds the CLI) works correctly with custom SSL certificates.

## What These Tests Verify

The tests communicate with the MCP server using the MCP protocol (JSON-RPC over stdio), invoking tools that use the embedded CLI:

1. **Environment Setup** - CA certificate file is accessible, environment variables are set correctly
2. **MCP Server Startup** - The MCP server starts and accepts protocol requests
3. **MCP Tool Invocation (valid cert)** - Tools work correctly with proper SSL configuration
4. **MCP Tool Invocation (no cert)** - SSL errors occur when no cert is provided
5. **MCP Tool Invocation (wrong cert)** - SSL errors occur when wrong cert is provided

## MCP Server Variants

There are two deployment variants of the MCP server, and both are tested:

### Docker Variant

Tests the actual Docker image as users run it:
- Builds `docker build -t codescene-mcp .`
- Runs via `docker run -i codescene-mcp` with SSL certs mounted
- Uses the same command structure users would use

### Static Binary Variant

Tests the Nuitka-compiled standalone binary:
- Builds `cs-mcp` using Nuitka
- Runs the binary directly
- Uses local SSL certificate configuration

## Architecture

```
┌─────────────────────┐  MCP Protocol   ┌─────────────────────────┐
│  Test Client        │ ◀─────────────▶ │  MCP Server             │
│  (Python script)    │   (stdio)       │  docker run -i or       │
│  runs on HOST       │                 │  ./cs-mcp binary        │
└─────────────────────┘                 └───────────┬─────────────┘
                                                    │ HTTPS
                                                    ▼
                                        ┌─────────────────────────┐
                                        │  nginx (Docker)         │
                                        │  Self-signed SSL cert   │
                                        │  Proxies to real backend│
                                        └───────────┬─────────────┘
                                                    │ HTTPS
                                                    ▼
                                        ┌─────────────────────────┐
                                        │  Backend                │
                                        │  codescene.io or        │
                                        │  on-prem instance       │
                                        └─────────────────────────┘
```

## Running the Tests

### Prerequisites

- Docker installed
- For static variant: Python 3.13, Nuitka

### Run Tests

```bash
cd tests/ssl-integration

# Test Docker variant (proxies to codescene.io)
./run-ssl-test.sh docker

# Test static binary variant (proxies to codescene.io)
./run-ssl-test.sh static

# Test Docker variant with on-prem backend
./run-ssl-test.sh docker-onprem

# Test static variant with on-prem backend
./run-ssl-test.sh static-onprem
```

### Test Variants

| Variant | Deployment | Backend |
|---------|------------|---------|
| `docker` | Docker image | codescene.io |
| `static` | Nuitka binary | codescene.io |
| `docker-onprem` | Docker image | test-env.enterprise.codescene.io |
| `static-onprem` | Nuitka binary | test-env.enterprise.codescene.io |

### Docker Variant - What Happens

1. Builds the MCP Docker image from the main Dockerfile
2. Starts nginx container with self-signed SSL certificate
3. Runs `docker run -i` with SSL certificates mounted
4. Test client on HOST sends MCP protocol requests via stdio
5. Verifies SSL works with valid cert, fails without cert or with wrong cert
6. Cleans up containers and images

### Static Variant - What Happens

1. Downloads CodeScene CLI if not present
2. Builds `cs-mcp` binary using Nuitka (takes several minutes)
3. Starts nginx with self-signed SSL cert via Docker
4. Runs the static binary and sends MCP protocol requests
5. Verifies SSL works with valid cert, fails without cert or with wrong cert
6. Cleans up

## Docker SSL Configuration

For production use with Docker, mount your CA certificate and set the environment variable:

```json
{
  "args": [
    "run", "-i", "--rm",
    "-e", "CS_ACCESS_TOKEN",
    "-e", "REQUESTS_CA_BUNDLE=/certs/ca-bundle.crt",
    "--mount", "type=bind,src=/path/to/ca-bundle.crt,dst=/certs/ca-bundle.crt,ro",
    "codescene/codescene-mcp"
  ]
}
```

See the [Docker installation docs](../../docs/docker-installation.md#custom-ssltls-certificates) for more details.
