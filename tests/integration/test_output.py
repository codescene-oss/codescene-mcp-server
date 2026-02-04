#!/usr/bin/env python3
"""
Test output formatting utilities.

This module provides functions for formatted test output including
headers, test results, and summaries.
"""

import sys


def _needs_ascii_fallback() -> bool:
    """Check if stdout needs ASCII fallback for encoding compatibility."""
    return sys.stdout.encoding and sys.stdout.encoding.lower() in ('cp1252', 'ascii')


def _get_status_text(passed: bool, use_ascii: bool) -> str:
    """Get status text for a test result."""
    if use_ascii:
        return "[PASS]" if passed else "[FAIL]"
    return "\u2713 PASS" if passed else "\u2717 FAIL"


def _get_status_color(passed: bool) -> str:
    """Get ANSI color code for a test result."""
    return "\033[92m" if passed else "\033[91m"


def _print_details(details: str) -> None:
    """Print test result details, limited to first 10 lines."""
    for line in details.split('\n')[:10]:
        print(f"         {line}")


def print_header(msg: str) -> None:
    """Print a formatted test section header."""
    print(f"\n{'='*70}")
    print(f"  {msg}")
    print(f"{'='*70}\n")


def print_test(name: str, passed: bool, details: str = "") -> None:
    """Print a test result."""
    status = _get_status_text(passed, _needs_ascii_fallback())
    color = _get_status_color(passed)
    reset = "\033[0m"
    print(f"  {color}{status}{reset}: {name}")
    if details:
        _print_details(details)


def print_summary(results: list[tuple[str, bool]]) -> int:
    """
    Print test summary and return exit code.
    
    Args:
        results: List of (test_name, passed) tuples
        
    Returns:
        0 if all tests passed, 1 otherwise
    """
    print_header("Test Summary")
    
    passed = [name for name, p in results if p]
    failed = [name for name, p in results if not p]
    
    print(f"  Total: {len(results)} tests")
    print(f"  \033[92mPassed: {len(passed)}\033[0m")
    if failed:
        print(f"  \033[91mFailed: {len(failed)}\033[0m")
        print("\n  Failed tests:")
        for name in failed:
            print(f"    - {name}")
    
    return 0 if len(failed) == 0 else 1
