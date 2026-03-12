#!/usr/bin/env bash
set -euo pipefail

ARCH="${1:-amd64}"
VERSION="${VERSION:-MCP-0.0.0-local}"

case "${ARCH}" in
  amd64)
    MANYLINUX_IMAGE="quay.io/pypa/manylinux_2_28_x86_64"
    PYTHON_BIN="/opt/python/cp313-cp313/bin/python"
    CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip"
    ARTIFACT_NAME="cs-mcp-linux-amd64"
    ;;
  aarch64)
    MANYLINUX_IMAGE="quay.io/pypa/manylinux_2_28_aarch64"
    PYTHON_BIN="/opt/python/cp313-cp313/bin/python"
    CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip"
    ARTIFACT_NAME="cs-mcp-linux-aarch64"
    ;;
  *)
    echo "Unsupported arch '${ARCH}'. Use amd64 or aarch64." >&2
    exit 1
    ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUILDER_IMAGE="codescene-mcp-linux-exe-builder:${ARCH}"

mkdir -p "${REPO_ROOT}/.home" "${REPO_ROOT}/.cache/Nuitka"

docker build \
  --platform "linux/${ARCH}" \
  -f "${REPO_ROOT}/docker/linux-exe-builder.Dockerfile" \
  --build-arg MANYLINUX_IMAGE="${MANYLINUX_IMAGE}" \
  --build-arg PYTHON_BIN="${PYTHON_BIN}" \
  -t "${BUILDER_IMAGE}" \
  "${REPO_ROOT}"

docker run --rm \
  --platform "linux/${ARCH}" \
  --user "$(id -u):$(id -g)" \
  -v "${REPO_ROOT}:/work" \
  -w /work \
  -e HOME=/work/.home \
  -e NUITKA_CACHE_DIR=/work/.cache/Nuitka \
  -e ROOT_DIR=/work \
  -e PYTHON_BIN="${PYTHON_BIN}" \
  -e VERSION="${VERSION}" \
  -e CLI_URL="${CLI_URL}" \
  -e OUTPUT_NAME=cs-mcp \
  -e ARTIFACT_NAME="${ARTIFACT_NAME}" \
  "${BUILDER_IMAGE}"

echo "Local build finished: ${REPO_ROOT}/${ARTIFACT_NAME}"
