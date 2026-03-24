#!/usr/bin/env python3
"""
Stress test code_health_review against a built cs-mcp binary.

The goal is to catch intermittent embedded `cs` CLI failures, especially
the telemetry flush race that can surface as:

  NoSuchFileException: .../codescene-cli.log.jsonl
"""

import argparse
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files
from test_utils import MCPClient, create_git_repo, create_test_environment, extract_result_text, safe_temp_directory

TELEMETRY_RACE_MARKERS = ("NoSuchFileException", "codescene-cli.log.jsonl")


def _contains_telemetry_race_error(text: str) -> bool:
    return all(marker in text for marker in TELEMETRY_RACE_MARKERS)


def _build_env() -> dict[str, str]:
    env = create_test_environment()
    env.setdefault("CS_DISABLE_VERSION_CHECK", "1")
    env.pop("CS_DISABLE_TRACKING", None)
    return env


def _collect_new_stderr(client: MCPClient, cursor: int) -> tuple[str, int]:
    lines = client.stderr_lines[cursor:]
    return "\n".join(lines), len(client.stderr_lines)


def run_stress_test(executable: Path, iterations: int, timeout: int) -> int:
    with safe_temp_directory(prefix="cs_mcp_review_stress_") as test_dir:
        repo_dir = create_git_repo(test_dir, get_sample_files())
        review_target = repo_dir / "src/services/order_processor.py"

        env = _build_env()
        command = [str(executable)]

        client = MCPClient(command, env=env, cwd=str(repo_dir))
        if not client.start():
            print("FAIL: could not start MCP server")
            stderr = client.get_stderr()
            if stderr:
                print("Recent stderr:\n" + stderr)
            return 1

        init_response = client.initialize()
        if "result" not in init_response:
            print("FAIL: initialize did not return a result")
            print(init_response)
            client.stop()
            return 1

        print(f"Running {iterations} code_health_review calls...")

        start = time.time()
        stderr_cursor = 0
        total_failures = 0
        telemetry_failures = 0

        try:
            for i in range(1, iterations + 1):
                response = client.call_tool(
                    "code_health_review",
                    {"file_path": str(review_target)},
                    timeout=timeout,
                )

                response_text = extract_result_text(response)
                new_stderr, stderr_cursor = _collect_new_stderr(client, stderr_cursor)

                failed = False
                if "error" in response:
                    failed = True
                elif not response_text:
                    failed = True

                telemetry_error = _contains_telemetry_race_error(response_text) or _contains_telemetry_race_error(
                    new_stderr
                )
                if telemetry_error:
                    telemetry_failures += 1
                    failed = True

                if failed:
                    total_failures += 1
                    print(f"[{i}/{iterations}] FAIL")
                    if telemetry_error:
                        print("  telemetry race marker detected")
                    if "error" in response:
                        print(f"  response error: {response['error']}")
                    if response_text:
                        preview = response_text[:300].replace("\n", " ")
                        print(f"  response preview: {preview}")
                    if new_stderr:
                        print("  new stderr lines:")
                        print("  " + "\n  ".join(new_stderr.splitlines()[-8:]))
                elif i % 25 == 0 or i == iterations:
                    print(f"[{i}/{iterations}] OK")
        finally:
            client.stop()

        elapsed = time.time() - start
        print("\n--- Stress Test Summary ---")
        print(f"iterations:         {iterations}")
        print(f"total failures:     {total_failures}")
        print(f"telemetry failures: {telemetry_failures}")
        print(f"elapsed seconds:    {elapsed:.1f}")

        if total_failures == 0:
            print("PASS: no failures observed")
            return 0

        if telemetry_failures > 0:
            print("FAIL: telemetry race still observed")
        else:
            print("FAIL: non-telemetry errors observed")
        return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Stress test code_health_review for CLI stability")
    parser.add_argument("executable", type=Path, help="Path to cs-mcp binary")
    parser.add_argument(
        "--iterations",
        type=int,
        default=250,
        help="Number of code_health_review calls (default: 250)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=90,
        help="Per-call timeout in seconds (default: 90)",
    )
    args = parser.parse_args()

    if not args.executable.exists():
        print(f"Error: executable not found: {args.executable}")
        return 1

    if not os.getenv("CS_ACCESS_TOKEN"):
        print("Error: CS_ACCESS_TOKEN is not set")
        return 1

    if args.iterations < 1:
        print("Error: --iterations must be >= 1")
        return 1

    return run_stress_test(args.executable, args.iterations, args.timeout)


if __name__ == "__main__":
    sys.exit(main())
