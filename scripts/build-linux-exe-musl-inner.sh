#!/usr/bin/env bash
set -euo pipefail

# Inner build script for musl-based portable Linux executables.
# Runs inside the Alpine Docker container built from
# docker/linux-exe-builder-musl.Dockerfile.
#
# Strategy:
#   1. Build with Nuitka --onefile (musl-linked, NOT fully static).
#      All libs except musl libc are linked statically via -l:libfoo.a.
#   2. Compile musl-launcher.c as a fully static binary.
#   3. Embed the musl dynamic linker + Nuitka onefile into the launcher
#      using objcopy, producing a single zero-dependency binary.

ROOT_DIR="${ROOT_DIR:-/work}"
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

# ---------- obtain CodeScene CLI ----------
if [ -n "${CLI_PATH}" ]; then
  cp "${CLI_PATH}" "${CLI_STAGE_PATH}"
  chmod +x "${CLI_STAGE_PATH}"
else
  rm -rf /tmp/codescene-cli
  mkdir -p /tmp/codescene-cli
  python3 - <<'PY'
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

# ---------- inject version ----------
ORIGINAL_VERSION_FILE_CONTENT="$(cat src/version.py)"
cleanup() {
  printf '%s' "${ORIGINAL_VERSION_FILE_CONTENT}" > src/version.py
}
trap cleanup EXIT

python3 - <<'PY'
from pathlib import Path
import os

version = os.environ.get("VERSION", "dev")
path = Path("src/version.py")
content = path.read_text()
path.write_text(content.replace('__version__ = "dev"', f'__version__ = "{version}"'))
print(f"Injected version: {version}")
PY

# ---------- Step 1: Build with Nuitka --onefile ----------
# We do NOT set LDFLAGS="-static" because that disables dlopen(), which
# Python needs to load C extension modules. Instead, we link specific
# libraries statically using the -l:libfoo.a syntax. This produces a
# binary that dynamically links ONLY against musl libc (ld-musl-*.so.1).
#
# The -l:libfoo.a syntax tells the linker to use the static archive
# even when a shared library is also available.
#
# The --dynamic-linker flag sets the ELF PT_INTERP to a custom path
# where the launcher will extract the musl linker at runtime. This is
# critical: without it, /proc/self/exe would point to the linker instead
# of the binary when invoked via "ld-musl ./binary", breaking Nuitka's
# onefile data payload lookup.
#
# Nuitka's onefile packager scans ELF dependencies and requires ALL
# referenced files to exist. Since /tmp/.csm/ld.so doesn't exist at
# build time, we create a symlink to the real musl linker so Nuitka
# can find it. The bundled copy is harmless — at runtime, the launcher
# extracts the real musl linker to /tmp/.csm/ld.so before executing.
mkdir -p /tmp/.csm
ln -sf /lib/ld-musl-*.so.1 /tmp/.csm/ld.so

export LDFLAGS="-Wl,--dynamic-linker=/tmp/.csm/ld.so -l:libssl.a -l:libcrypto.a -l:libz.a -l:libyaml.a -l:libbz2.a -l:libsqlite3.a -l:libreadline.a -l:libncursesw.a -l:libffi.a"

# Verify the static libpython exists.
STATIC_LIBPYTHON="$(find /opt/python/lib -name 'libpython3.13*.a' -print -quit 2>/dev/null || true)"
if [ -z "${STATIC_LIBPYTHON}" ]; then
  STATIC_LIBPYTHON="$(find /build/cpython -name 'libpython3.13*.a' -print -quit 2>/dev/null || true)"
fi
if [ -z "${STATIC_LIBPYTHON}" ]; then
  echo "ERROR: Could not find static libpython3.13.a" >&2
  exit 1
fi
echo "Found static libpython: ${STATIC_LIBPYTHON}"

INNER_NAME="cs-mcp-inner"

echo ""
echo "=== Step 1/3: Building Nuitka onefile (musl-linked) ==="
echo ""

python3 -m nuitka --onefile \
  --assume-yes-for-downloads \
  --lto=yes \
  --static-libpython=yes \
  --company-name="CodeScene AB" \
  --product-name="CodeHealth MCP" \
  --noinclude-pytest-mode=nofollow \
  --noinclude-unittest-mode=nofollow \
  --noinclude-setuptools-mode=nofollow \
  --noinclude-pydoc-mode=nofollow \
  --include-data-dir=./src/docs=src/docs \
  --include-data-dir=./src/code_health_refactoring_business_case/s_curve/regression=code_health_refactoring_business_case/s_curve/regression \
  --include-data-files="${CLI_STAGE_PATH}=cs" \
  --output-filename="${INNER_NAME}" \
  src/cs_mcp_server.py

chmod +x "${INNER_NAME}"

echo ""
echo "Inner binary info:"
file "${INNER_NAME}"
echo "ELF interpreter (should be /tmp/.csm/ld.so):"
readelf -l "${INNER_NAME}" 2>/dev/null | grep -A1 INTERP || echo "(readelf not available)"
echo "Dynamic deps:"
ldd "${INNER_NAME}" 2>&1 || echo "(ldd exit code: $?)"
echo ""

# ---------- Step 2: Locate musl dynamic linker ----------
echo "=== Step 2/3: Preparing musl linker payload ==="

# Find the musl dynamic linker
if [ -f /lib/ld-musl-x86_64.so.1 ]; then
  LD_MUSL_PATH="/lib/ld-musl-x86_64.so.1"
elif [ -f /lib/ld-musl-aarch64.so.1 ]; then
  LD_MUSL_PATH="/lib/ld-musl-aarch64.so.1"
else
  echo "ERROR: Could not find musl dynamic linker" >&2
  ls -la /lib/ld-musl-* 2>/dev/null || echo "No ld-musl-* found in /lib/"
  exit 1
fi
echo "Using musl linker: ${LD_MUSL_PATH}"

# Copy payloads to temp build dir with predictable names for objcopy
BUILD_TMP="/tmp/launcher-build"
rm -rf "${BUILD_TMP}"
mkdir -p "${BUILD_TMP}"

cp "${LD_MUSL_PATH}" "${BUILD_TMP}/ld_musl_so"
cp "${INNER_NAME}" "${BUILD_TMP}/cs_mcp_inner"

# ---------- Step 3: Build static launcher with embedded payloads ----------
echo ""
echo "=== Step 3/3: Building static launcher ==="

# Convert payloads to linkable object files using objcopy.
# This creates symbols like _binary_ld_musl_so_start, _binary_ld_musl_so_end, etc.
cd "${BUILD_TMP}"

# Detect architecture for objcopy output format
MACHINE="$(uname -m)"
case "${MACHINE}" in
  x86_64)
    OBJCOPY_FMT="elf64-x86-64"
    OBJCOPY_ARCH="i386:x86-64"
    ;;
  aarch64)
    OBJCOPY_FMT="elf64-littleaarch64"
    OBJCOPY_ARCH="aarch64"
    ;;
  *)
    echo "ERROR: Unsupported architecture: ${MACHINE}" >&2
    exit 1
    ;;
esac

objcopy -I binary -O "${OBJCOPY_FMT}" -B "${OBJCOPY_ARCH}" \
  --rename-section .data=.rodata,alloc,load,readonly,data,contents \
  ld_musl_so ld_musl_so.o

objcopy -I binary -O "${OBJCOPY_FMT}" -B "${OBJCOPY_ARCH}" \
  --rename-section .data=.rodata,alloc,load,readonly,data,contents \
  cs_mcp_inner cs_mcp_inner.o

# Compile and link the launcher statically.
# The launcher itself is fully static — no interpreter, no shared libs.
gcc -static -O2 -Wall -Wextra \
  -o "${ROOT_DIR}/${OUTPUT_NAME}" \
  /build/musl-launcher.c \
  ld_musl_so.o \
  cs_mcp_inner.o

cd "${ROOT_DIR}"
chmod +x "${OUTPUT_NAME}"
rm -rf "${BUILD_TMP}"

# ---------- verify the final binary ----------
echo ""
echo "=== Final binary verification ==="
file "${OUTPUT_NAME}"
ls -lh "${OUTPUT_NAME}"
echo ""

# ldd on a static binary prints "not a dynamic executable" (glibc) or
# "Not a valid dynamic program" (musl) and exits non-zero.
LDD_OUTPUT="$(ldd "${OUTPUT_NAME}" 2>&1 || true)"
if echo "${LDD_OUTPUT}" | grep -qi "statically linked\|not a dynamic\|not a valid dynamic"; then
  echo "PASS: Launcher binary is statically linked (zero dependencies)"
else
  echo "WARNING: Launcher may still have dynamic dependencies:"
  echo "${LDD_OUTPUT}"
fi

if [ -n "${ARTIFACT_NAME}" ]; then
  mv "${OUTPUT_NAME}" "${ARTIFACT_NAME}"
  OUTPUT_NAME="${ARTIFACT_NAME}"
fi

cp "${CLI_STAGE_PATH}" ./cs-linux-bundled
chmod +x ./cs-linux-bundled

# Clean up the intermediate inner binary
rm -f "${INNER_NAME}"

# Fix ownership of output files so they aren't root-owned on the host
if [ -n "${FIX_UID:-}" ] && [ -n "${FIX_GID:-}" ]; then
  chown "${FIX_UID}:${FIX_GID}" "${OUTPUT_NAME}" ./cs-linux-bundled 2>/dev/null || true
fi

echo ""
echo "Built portable executable: ${ROOT_DIR}/${OUTPUT_NAME}"
echo ""
echo "This binary has ZERO runtime dependencies and will run on any Linux"
echo "distribution (Alpine, Ubuntu, Debian, RHEL, Amazon Linux, etc.)."
