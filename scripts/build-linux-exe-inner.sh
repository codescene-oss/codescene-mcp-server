#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${ROOT_DIR:-/work}"
PYTHON_BIN="${PYTHON_BIN:-/opt/python/cp313-cp313/bin/python}"
VERSION="${VERSION:-dev}"
CLI_PATH="${CLI_PATH:-}"
CLI_URL="${CLI_URL:-https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip}"
OUTPUT_NAME="${OUTPUT_NAME:-cs-mcp}"
ARTIFACT_NAME="${ARTIFACT_NAME:-}"
CLI_STAGE_PATH="/tmp/codescene-cli/cs"

mkdir -p /tmp/codescene-cli

cd "${ROOT_DIR}"

if [ ! -f "src/cs_mcp_server.py" ]; then
  echo "Expected repository root at ${ROOT_DIR}, missing src/cs_mcp_server.py" >&2
  exit 1
fi

if [ -n "${CLI_PATH}" ]; then
  cp "${CLI_PATH}" "${CLI_STAGE_PATH}"
  chmod +x "${CLI_STAGE_PATH}"
else
  rm -rf /tmp/codescene-cli
  mkdir -p /tmp/codescene-cli
  "${PYTHON_BIN}" - <<'PY'
import os
import pathlib
import urllib.request
import zipfile

url = os.environ["CLI_URL"]
zip_path = pathlib.Path("/tmp/codescene-cli.zip")
dest = pathlib.Path("/tmp/codescene-cli")

urllib.request.urlretrieve(url, zip_path)
with zipfile.ZipFile(zip_path, "r") as zf:
    zf.extractall(dest)
PY
  chmod +x "${CLI_STAGE_PATH}"
fi

ORIGINAL_VERSION_FILE_CONTENT="$(cat src/version.py)"
cleanup() {
  printf '%s' "${ORIGINAL_VERSION_FILE_CONTENT}" > src/version.py
}
trap cleanup EXIT

"${PYTHON_BIN}" - <<'PY'
from pathlib import Path
import os

version = os.environ.get("VERSION", "dev")
path = Path("src/version.py")
content = path.read_text()
path.write_text(content.replace('__version__ = "dev"', f'__version__ = "{version}"'))
print(f"Injected version: {version}")
PY

"${PYTHON_BIN}" -m nuitka --onefile \
  --assume-yes-for-downloads \
  --lto=yes \
  --company-name="CodeScene AB" \
  --product-name="CodeHealth MCP" \
  --noinclude-pytest-mode=nofollow \
  --noinclude-unittest-mode=nofollow \
  --noinclude-setuptools-mode=nofollow \
  --noinclude-pydoc-mode=nofollow \
  --include-module=lupa.lua51 \
  --include-data-dir=./src/docs=src/docs \
  --include-data-dir=./src/code_health_refactoring_business_case/s_curve/regression=code_health_refactoring_business_case/s_curve/regression \
  --include-data-files="${CLI_STAGE_PATH}=cs" \
  --output-filename="${OUTPUT_NAME}" \
  src/cs_mcp_server.py

chmod +x "${OUTPUT_NAME}"

if [ -n "${ARTIFACT_NAME}" ]; then
  mv "${OUTPUT_NAME}" "${ARTIFACT_NAME}"
  OUTPUT_NAME="${ARTIFACT_NAME}"
fi

cp "${CLI_STAGE_PATH}" ./cs-linux-bundled
chmod +x ./cs-linux-bundled

echo "Built executable: ${ROOT_DIR}/${OUTPUT_NAME}"
