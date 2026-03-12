#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path


def parse_version(version: str) -> tuple[int, int]:
    major, minor = version.split(".")
    return int(major), int(minor)


def extract_glibc_versions(binary_path: Path) -> list[str]:
    try:
        output = subprocess.check_output(
            ["readelf", "-V", str(binary_path)],
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except (FileNotFoundError, subprocess.CalledProcessError):
        return []

    versions = set(re.findall(r"Name: GLIBC_(\d+\.\d+)", output))
    return sorted(versions, key=parse_version)


def validate_binary(binary_path: Path, max_glibc: str) -> bool:
    versions = extract_glibc_versions(binary_path)
    if not versions:
        print(f"[glibc-check] {binary_path}: no GLIBC symbol versions found (likely static/non-glibc)")
        return True

    max_found = versions[-1]
    print(f"[glibc-check] {binary_path}: max GLIBC_{max_found}")
    if parse_version(max_found) > parse_version(max_glibc):
        print(
            f"[glibc-check] FAIL: {binary_path} requires GLIBC_{max_found}, "
            f"which exceeds allowed GLIBC_{max_glibc}",
            file=sys.stderr,
        )
        return False

    return True


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate max required GLIBC version for ELF binaries")
    parser.add_argument("binaries", nargs="+", help="Binary paths to inspect")
    parser.add_argument("--max", default="2.28", dest="max_glibc", help="Maximum allowed GLIBC version")
    args = parser.parse_args()

    ok = True
    for binary in args.binaries:
        path = Path(binary)
        if not path.exists():
            print(f"[glibc-check] FAIL: missing binary: {path}", file=sys.stderr)
            ok = False
            continue
        ok = validate_binary(path, args.max_glibc) and ok

    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
