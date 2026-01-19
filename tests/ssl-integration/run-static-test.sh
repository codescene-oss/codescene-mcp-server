#!/bin/bash
# Static Binary SSL Integration Test Runner
#
# This script:
# 1. Builds the cs-mcp static binary using Nuitka
# 2. Starts nginx with self-signed SSL cert via Docker
# 3. Runs the static binary and sends MCP protocol requests
# 4. Verifies SSL works end-to-end
# 5. Cleans up
#
# Prerequisites:
# - Python 3.13
# - Nuitka installed (pip install Nuitka)
# - Docker for nginx
# - CodeScene CLI (cs) for the platform

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

echo "============================================================"
echo "  Static Binary SSL Integration Test"
echo "  Testing: cs-mcp binary with SSL certificates"
echo "============================================================"
echo ""

# Create temp directory for build artifacts
BUILD_DIR=$(mktemp -d)
CERT_DIR="$BUILD_DIR/certs"
mkdir -p "$CERT_DIR"

cleanup() {
    echo ""
    echo "Cleaning up..."
    
    # Stop nginx container
    docker rm -f ssl-test-nginx 2>/dev/null || true
    
    # Remove build artifacts
    rm -rf "$BUILD_DIR"
    rm -f "$REPO_ROOT/cs-mcp" "$REPO_ROOT/cs-mcp.bin" 2>/dev/null || true
    
    # Remove Nuitka build directories
    rm -rf "$REPO_ROOT/cs_mcp_server.build" 2>/dev/null || true
    rm -rf "$REPO_ROOT/cs_mcp_server.dist" 2>/dev/null || true
    rm -rf "$REPO_ROOT/cs_mcp_server.onefile-build" 2>/dev/null || true
    
    echo "Cleanup complete."
}

trap cleanup EXIT

# Step 1: Check prerequisites
echo "Step 1: Checking prerequisites..."

if ! command -v python3.13 &> /dev/null; then
    echo "  ✗ Python 3.13 not found"
    echo "  Install Python 3.13 to run static variant tests"
    exit 1
fi
echo "  ✓ Python 3.13 found"

if ! python3.13 -c "import nuitka" 2>/dev/null; then
    echo "  ✗ Nuitka not found"
    echo "  Install with: pip install Nuitka"
    exit 1
fi
echo "  ✓ Nuitka found"

# Step 2: Download CS CLI if not present
echo ""
echo "Step 2: Checking CodeScene CLI..."

if [ ! -f "$REPO_ROOT/cs" ]; then
    echo "  Downloading CodeScene CLI..."
    
    # Detect platform
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if [[ $(uname -m) == "arm64" ]]; then
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip"
        else
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip"
        fi
    elif [[ "$OSTYPE" == "linux"* ]]; then
        if [[ $(uname -m) == "aarch64" ]]; then
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip"
        else
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip"
        fi
    else
        echo "  ✗ Unsupported platform: $OSTYPE"
        exit 1
    fi
    
    curl -fsSL "$CLI_URL" -o "$BUILD_DIR/cs.zip"
    unzip -q "$BUILD_DIR/cs.zip" -d "$REPO_ROOT"
    chmod +x "$REPO_ROOT/cs"
    echo "  ✓ CodeScene CLI downloaded"
else
    echo "  ✓ CodeScene CLI found"
fi

# Step 3: Build cs-mcp static binary
echo ""
echo "Step 3: Building cs-mcp static binary..."
echo "  This may take several minutes..."

cd "$REPO_ROOT"

# Create virtual environment if needed
if [ ! -d ".venv" ]; then
    python3.13 -m venv .venv
fi
source .venv/bin/activate

# Install dependencies
pip install -q -r src/requirements.txt
pip install -q Nuitka

# Build with Nuitka
python3.13 -m nuitka --onefile \
    --assume-yes-for-downloads \
    --include-data-dir=./src/docs=src/docs \
    --include-data-files=./cs=cs \
    --output-filename=cs-mcp \
    src/cs_mcp_server.py 2>&1 | tail -5

if [ ! -f "$REPO_ROOT/cs-mcp" ]; then
    echo "  ✗ Failed to build cs-mcp binary"
    exit 1
fi
echo "  ✓ cs-mcp binary built successfully"

# Step 4: Generate SSL certificate
echo ""
echo "Step 4: Generating SSL certificate..."

openssl req -x509 -nodes -days 1 -newkey rsa:2048 \
    -keyout "$CERT_DIR/server.key" \
    -out "$CERT_DIR/ca.crt" \
    -subj "/C=US/ST=Test/L=Test/O=Test/CN=localhost" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" 2>/dev/null

echo "  ✓ Certificate generated"

# Step 5: Start nginx SSL proxy
echo ""
echo "Step 5: Starting nginx SSL proxy..."

# Create nginx config that proxies to real CodeScene API
cat > "$BUILD_DIR/nginx.conf" << 'EOF'
events { worker_connections 1024; }
http {
    # Resolver for DNS lookups (Docker's embedded DNS + public DNS)
    resolver 8.8.8.8 1.1.1.1 valid=300s;
    resolver_timeout 5s;
    
    server {
        listen 8443 ssl;
        ssl_certificate /certs/ca.crt;
        ssl_certificate_key /certs/server.key;
        
        location /health {
            return 200 'ok';
            add_header Content-Type text/plain;
        }
        
        # Proxy all API requests to real CodeScene
        location / {
            proxy_pass https://codescene.io;
            proxy_ssl_server_name on;
            proxy_set_header Host codescene.io;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            
            # Timeouts
            proxy_connect_timeout 30s;
            proxy_send_timeout 30s;
            proxy_read_timeout 30s;
        }
    }
}
EOF

docker run -d --name ssl-test-nginx \
    -p 8443:8443 \
    -v "$CERT_DIR:/certs:ro" \
    -v "$BUILD_DIR/nginx.conf:/etc/nginx/nginx.conf:ro" \
    nginx:alpine

# Wait for nginx to be ready
sleep 2
if curl -sk https://localhost:8443/health > /dev/null 2>&1; then
    echo "  ✓ nginx SSL proxy started"
else
    echo "  ✗ nginx failed to start"
    docker logs ssl-test-nginx 2>&1 | tail -10
    exit 1
fi

# Step 6: Run the static binary test
echo ""
echo "Step 6: Running MCP protocol tests..."

export REQUESTS_CA_BUNDLE="$CERT_DIR/ca.crt"
export CS_ONPREM_URL="https://localhost:8443"
export CS_ACCESS_TOKEN="test-token"

# Run the test Python script
python3.13 "$SCRIPT_DIR/test_static_variant.py" "$REPO_ROOT/cs-mcp"
TEST_EXIT_CODE=$?

deactivate 2>/dev/null || true

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo ""
    echo "============================================================"
    echo "  Static Variant Tests PASSED ✓"
    echo "============================================================"
else
    echo ""
    echo "============================================================"
    echo "  Static Variant Tests FAILED ✗"
    echo "============================================================"
fi

exit $TEST_EXIT_CODE
