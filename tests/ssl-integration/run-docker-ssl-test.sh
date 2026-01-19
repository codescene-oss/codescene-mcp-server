#!/bin/bash
# Docker Image SSL Integration Test Runner
#
# This script tests the ACTUAL Docker image as users run it:
# 1. Builds the Docker image from the main Dockerfile
# 2. Starts nginx with self-signed SSL cert
# 3. Runs `docker run -i codescene-mcp` with SSL certs mounted
# 4. Sends MCP protocol requests via stdio
# 5. Verifies SSL works end-to-end
#
# Environment variables (set by run-ssl-test.sh):
# - BACKEND_HOST: The host to proxy to (e.g., codescene.io)
# - BACKEND_URL: The full URL (e.g., https://codescene.io)
#
# Prerequisites:
# - Docker installed

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

# Default backend if not set by parent script
BACKEND_HOST="${BACKEND_HOST:-codescene.io}"
BACKEND_URL="${BACKEND_URL:-https://codescene.io}"

# Docker image name for testing
MCP_IMAGE="codescene-mcp-ssl-test"
NGINX_CONTAINER="ssl-test-nginx-docker"
NETWORK_NAME="ssl-test-network"

echo "============================================================"
echo "  Docker Image SSL Integration Test"
echo "  Testing: docker run with SSL certificates"
echo "  Backend: $BACKEND_URL"
echo "============================================================"
echo ""

# Create temp directory for certs
BUILD_DIR=$(mktemp -d)
CERT_DIR="$BUILD_DIR/certs"
mkdir -p "$CERT_DIR"

cleanup() {
    echo ""
    echo "Cleaning up..."
    
    # Stop nginx container
    docker rm -f "$NGINX_CONTAINER" 2>/dev/null || true
    
    # Remove network
    docker network rm "$NETWORK_NAME" 2>/dev/null || true
    
    # Remove test image
    docker rmi "$MCP_IMAGE" 2>/dev/null || true
    
    # Remove temp directory
    rm -rf "$BUILD_DIR"
    
    echo "Cleanup complete."
}

trap cleanup EXIT

# Clean up any leftover resources from previous runs
echo "Cleaning up any leftover resources..."
docker rm -f "$NGINX_CONTAINER" 2>/dev/null || true
docker network rm "$NETWORK_NAME" 2>/dev/null || true
echo ""

# Step 1: Build the Docker image
echo "Step 1: Building Docker image from main Dockerfile..."

docker build -t "$MCP_IMAGE" "$REPO_ROOT" 2>&1 | tail -5

if ! docker image inspect "$MCP_IMAGE" > /dev/null 2>&1; then
    echo "  ✗ Failed to build Docker image"
    exit 1
fi
echo "  ✓ Docker image built successfully"

# Step 2: Create Docker network
echo ""
echo "Step 2: Creating Docker network..."

docker network create "$NETWORK_NAME" 2>/dev/null || true
echo "  ✓ Network created"

# Step 3: Generate SSL certificate
echo ""
echo "Step 3: Generating SSL certificate..."

openssl req -x509 -nodes -days 1 -newkey rsa:2048 \
    -keyout "$CERT_DIR/server.key" \
    -out "$CERT_DIR/ca.crt" \
    -subj "/C=US/ST=Test/L=Test/O=Test/CN=$NGINX_CONTAINER" \
    -addext "subjectAltName=DNS:$NGINX_CONTAINER,DNS:localhost,IP:127.0.0.1" 2>/dev/null

echo "  ✓ Certificate generated"

# Step 4: Start nginx SSL proxy
echo ""
echo "Step 4: Starting nginx SSL proxy..."

# Create nginx config
cat > "$BUILD_DIR/nginx.conf" << EOF
events { worker_connections 1024; }
http {
    resolver 8.8.8.8 1.1.1.1 valid=300s;
    resolver_timeout 5s;
    
    server {
        listen 443 ssl;
        ssl_certificate /certs/ca.crt;
        ssl_certificate_key /certs/server.key;
        
        location /health {
            return 200 'ok';
            add_header Content-Type text/plain;
        }
        
        location / {
            proxy_pass https://${BACKEND_HOST};
            proxy_ssl_server_name on;
            proxy_set_header Host ${BACKEND_HOST};
            proxy_set_header X-Real-IP \$remote_addr;
            proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto \$scheme;
            proxy_connect_timeout 30s;
            proxy_send_timeout 30s;
            proxy_read_timeout 30s;
        }
    }
}
EOF

docker run -d --name "$NGINX_CONTAINER" \
    --network "$NETWORK_NAME" \
    -v "$CERT_DIR:/certs:ro" \
    -v "$BUILD_DIR/nginx.conf:/etc/nginx/nginx.conf:ro" \
    nginx:alpine

# Wait for nginx to be ready
sleep 2
if docker exec "$NGINX_CONTAINER" curl -sk https://localhost/health > /dev/null 2>&1; then
    echo "  ✓ nginx SSL proxy started"
else
    echo "  ✗ nginx failed to start"
    docker logs "$NGINX_CONTAINER" 2>&1 | tail -10
    exit 1
fi

# Step 5: Run MCP protocol tests
echo ""
echo "Step 5: Running MCP protocol tests..."

# Export cert path for the test script
export CERT_PATH="$CERT_DIR/ca.crt"
export DOCKER_IMAGE="$MCP_IMAGE"
export DOCKER_NETWORK="$NETWORK_NAME"
export NGINX_HOST="$NGINX_CONTAINER"
export TEST_DATA_PATH="$REPO_ROOT/src/test_data"

# Run the test Python script (runs on host, uses docker run)
python3 "$SCRIPT_DIR/test_docker_run.py"
TEST_EXIT_CODE=$?

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo ""
    echo "============================================================"
    echo "  Docker Variant Tests PASSED ✓"
    echo "============================================================"
else
    echo ""
    echo "============================================================"
    echo "  Docker Variant Tests FAILED ✗"
    echo "============================================================"
fi

exit $TEST_EXIT_CODE
