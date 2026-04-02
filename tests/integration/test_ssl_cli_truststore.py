#!/usr/bin/env python3
"""
SSL truststore end-to-end integration tests.

These tests validate the full MCP -> embedded CLI argument path for SSL:
1. When REQUESTS_CA_BUNDLE is set, MCP injects Java truststore args.
2. When REQUESTS_CA_BUNDLE is missing, truststore args are not injected.

The test uses a fake CLI binary (CS_CLI_PATH) that verifies whether
`-Djavax.net.ssl.trustStore=...` is present and points to an existing file.
"""

import os
import stat
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    DockerBackend,
    MCPClient,
    CargoBackend,
    ServerBackend,
    create_git_repo,
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)


TEST_CA_CERT_PEM = """-----BEGIN CERTIFICATE-----
MIIDPzCCAiegAwIBAgIUdGj465l77xx7Je8KqOESIqx9zXYwDQYJKoZIhvcNAQEL
BQAwTzELMAkGA1UEBhMCVVMxDTALBgNVBAgMBFRlc3QxDTALBgNVBAcMBFRlc3Qx
EDAOBgNVBAoMB1Rlc3QgQ0ExEDAOBgNVBAMMB1Rlc3QgQ0EwHhcNMjYwMTE2MDky
OTQ5WhcNMjcwMTE2MDkyOTQ5WjBPMQswCQYDVQQGEwJVUzENMAsGA1UECAwEVGVz
dDENMAsGA1UEBwwEVGVzdDEQMA4GA1UECgwHVGVzdCBDQTEQMA4GA1UEAwwHVGVz
dCBDQTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMqoClSXXim/fiI9
Lc3X/4D4rHK6cWAnKVPA+CetSJiGrMrfeJZMSTWUv19M8aKlmbZsQxN4X4neycWE
UxH9y3XaqV9grmGvutTgw98t6fhawevGrjmcA+ygQ5S37reRQOHtc9ob51b8b9Rr
nyE8qIU2dkZ115VpFN+/woG2LG23iGj2dJ3AaZc/R8X0UQu5tQCDwTOeO/zMWPGG
xjzDpnFs4u7IAwPECEgEuxHH8PHapUoc0d+Aq/wBKM015qdohoaydrztzXp6DKJ5
RBv/cn+lTpFdvJQS0CceIo+hOUa46ONq63VM3SQhT7enOWToONBxrZpof18bITFd
2h4XxoMCAwEAAaMTMBEwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOC
AQEAHDWTjJILOtrCBRFksVyvniUGFR8ioz2cE4R8xcKAFxNOPKLuxwm+ilbUBX3A
8VOCJjR6IimsLMhAUEi5FGDiVVhOwIp1+pULEigTG7r72yOCr2xnw8NrX9UbJNnx
rlyCjEN9URBpriiGGegixH6AoLVW0SjEsJ7CgfqmfWzKU+nsPIunvePtFhSw5jHC
mHwYTxYcxYW33TK9qQxs119A9+qG5Z+cJlDtYrfHirHwPZQeuQ25jhKE5FUUiuiq
iblIIstcPF4n6wQ0ieNajmj5nHXQEypkek8D/ANbwwhlVQ3u/hldcAyj4qD7G5oJ
sC0Nc9QdNQt5Tos5Je5S7CWL0w==
-----END CERTIFICATE-----
"""


FAKE_CLI_SH = """#!/bin/sh
set -eu

cmd=""
has_truststore=0

for arg in "$@"; do
  case "$arg" in
    -Djavax.net.ssl.trustStore=*)
      ts="${arg#-Djavax.net.ssl.trustStore=}"
      if [ ! -f "$ts" ]; then
        echo "truststore file missing: $ts" 1>&2
        exit 21
      fi
      has_truststore=1
      ;;
    -D*)
      ;;
    *)
      if [ -z "$cmd" ]; then
        cmd="$arg"
      fi
      ;;
  esac
done

if [ "${REQUIRE_TRUSTSTORE:-0}" = "1" ] && [ "$has_truststore" -ne 1 ]; then
  echo "missing truststore arg" 1>&2
  exit 22
fi

if [ "$cmd" = "version" ]; then
  echo "fake-cli-version"
  exit 0
fi

if [ "$cmd" = "review" ]; then
  echo '{"score":9.5,"review":[]}'
  exit 0
fi

echo "unsupported command: $cmd" 1>&2
exit 23
"""


def run_ssl_cli_truststore_tests(executable: Path) -> int:
    backend = CargoBackend(executable=executable)
    return run_ssl_cli_truststore_tests_with_backend(backend)


def run_ssl_cli_truststore_tests_with_backend(backend: ServerBackend) -> int:
    if isinstance(backend, DockerBackend):
        return print_summary([
            ("SSL truststore args injected with REQUESTS_CA_BUNDLE", "SKIPPED"),
            ("SSL truststore args omitted without REQUESTS_CA_BUNDLE", "SKIPPED"),
        ])

    with safe_temp_directory(prefix="cs_mcp_ssl_cli_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")
        repo_dir = create_git_repo(test_dir, get_sample_files())

        fake_cli = _make_fake_cli(test_dir)
        cert_path = test_dir / "internal-ca.pem"
        cert_path.write_text(TEST_CA_CERT_PEM)

        command = backend.get_command(repo_dir)
        base_env = backend.get_env(os.environ.copy(), repo_dir)
        base_env["CS_ACCESS_TOKEN"] = base_env.get("CS_ACCESS_TOKEN", "test-token")
        base_env["CS_CLI_PATH"] = str(fake_cli)
        base_env["REQUIRE_TRUSTSTORE"] = "1"
        base_env["CS_DISABLE_VERSION_CHECK"] = "1"
        base_env["CS_DISABLE_TRACKING"] = "1"

        results = [
            (
                "SSL truststore args injected with REQUESTS_CA_BUNDLE",
                test_truststore_args_are_injected(command, base_env, repo_dir, cert_path),
            ),
            (
                "SSL truststore args omitted without REQUESTS_CA_BUNDLE",
                test_truststore_args_missing_without_cert(command, base_env, repo_dir),
            ),
        ]

        return print_summary(results)


def _make_fake_cli(test_dir: Path) -> Path:
    fake_cli = test_dir / "cs"
    fake_cli.write_text(FAKE_CLI_SH)
    mode = fake_cli.stat().st_mode
    fake_cli.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return fake_cli


def _call_score_tool(command: list[str], env: dict[str, str], repo_dir: Path) -> str:
    client = MCPClient(command, env=env, cwd=str(repo_dir))
    try:
        if not client.start():
            return f"START_ERROR: {client.get_stderr()}"
        client.initialize()
        target = repo_dir / "src/utils/calculator.py"
        response = client.call_tool("code_health_score", {"file_path": str(target)}, timeout=60)
        return extract_result_text(response)
    finally:
        client.stop()


def test_truststore_args_are_injected(
    command: list[str], env: dict[str, str], repo_dir: Path, cert_path: Path,
) -> bool:
    print_header("Test: Truststore Args Injected")
    case_env = env.copy()
    case_env["REQUESTS_CA_BUNDLE"] = str(cert_path)
    case_env.pop("SSL_CERT_FILE", None)
    case_env.pop("CURL_CA_BUNDLE", None)

    result_text = _call_score_tool(command, case_env, repo_dir)
    score = extract_code_health_score(result_text)
    ok = score is not None
    print_test("Code Health score returned", ok, result_text[:200])
    return ok


def test_truststore_args_missing_without_cert(
    command: list[str], env: dict[str, str], repo_dir: Path,
) -> bool:
    print_header("Test: Truststore Args Omitted Without Cert")
    case_env = env.copy()
    case_env.pop("REQUESTS_CA_BUNDLE", None)
    case_env.pop("SSL_CERT_FILE", None)
    case_env.pop("CURL_CA_BUNDLE", None)

    result_text = _call_score_tool(command, case_env, repo_dir)
    has_expected_failure = "missing truststore arg" in result_text.lower()
    print_test("Fails with missing truststore arg", has_expected_failure, result_text[:200])
    return has_expected_failure


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_ssl_cli_truststore.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("SSL Truststore CLI Integration Tests")
    return run_ssl_cli_truststore_tests(executable)


if __name__ == "__main__":
    sys.exit(main())
