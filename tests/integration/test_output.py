#!/usr/bin/env python3
"""
Test output formatting utilities.

This module provides functions for formatted test output including
headers, test results, and summaries.
"""

import sys
from dataclasses import dataclass

# ANSI color codes
COLOR_GREEN = "\033[92m"
COLOR_RED = "\033[91m"
COLOR_YELLOW = "\033[93m"
COLOR_RESET = "\033[0m"


def _needs_ascii_fallback() -> bool:
    """Check if stdout needs ASCII fallback for encoding compatibility."""
    encoding = sys.stdout.encoding
    if not encoding:
        return False
    return encoding.lower() in ("cp1252", "ascii")


def _get_status_text(passed: bool, use_ascii: bool) -> str:
    """Get status text for a test result."""
    if use_ascii:
        return "[PASS]" if passed else "[FAIL]"
    return "\u2713 PASS" if passed else "\u2717 FAIL"


def _get_status_color(passed: bool) -> str:
    """Get ANSI color code for a test result."""
    return COLOR_GREEN if passed else COLOR_RED


def _print_details(details: str) -> None:
    """Print test result details, limited to first 10 lines."""
    for line in details.split("\n")[:10]:
        print(f"         {line}")


def print_header(msg: str) -> None:
    """Print a formatted test section header."""
    print(f"\n{'=' * 70}")
    print(f"  {msg}")
    print(f"{'=' * 70}\n")


def print_test(name: str, passed: bool, details: str = "") -> None:
    """Print a test result."""
    status = _get_status_text(passed, _needs_ascii_fallback())
    color = _get_status_color(passed)
    print(f"  {color}{status}{COLOR_RESET}: {name}")
    if details:
        _print_details(details)


@dataclass
class TestCounts:
    """Categorized test result counts."""

    passed: list[str]
    failed: list[str]
    skipped: list[str]

    @classmethod
    def from_results(cls, results: list[tuple[str, bool | str]]) -> "TestCounts":
        """Categorize test results into passed, failed, and skipped."""
        return cls(
            passed=[name for name, result in results if result is True],
            failed=[name for name, result in results if result is False],
            skipped=[name for name, result in results if result == "SKIPPED"],
        )

    @property
    def total(self) -> int:
        """Total number of tests."""
        return len(self.passed) + len(self.failed) + len(self.skipped)


def _print_test_list(header: str, tests: list[str]) -> None:
    """Print a list of test names with a header."""
    print(f"\n  {header}:")
    for name in tests:
        print(f"    - {name}")


def _print_counts(counts: TestCounts) -> None:
    """Print the test count summary."""
    print(f"  Total: {counts.total} tests")
    print(f"  {COLOR_GREEN}Passed: {len(counts.passed)}{COLOR_RESET}")
    if counts.skipped:
        print(f"  {COLOR_YELLOW}Skipped: {len(counts.skipped)}{COLOR_RESET}")
    if counts.failed:
        print(f"  {COLOR_RED}Failed: {len(counts.failed)}{COLOR_RESET}")


def print_summary(results: list[tuple[str, bool | str]]) -> int:
    """
    Print test summary and return exit code.

    Args:
        results: List of (test_name, result) tuples where result is:
                 - True: test passed
                 - False: test failed
                 - "SKIPPED": test was skipped

    Returns:
        0 if all non-skipped tests passed, 1 otherwise
    """
    print_header("Test Summary")

    counts = TestCounts.from_results(results)
    _print_counts(counts)

    if counts.failed:
        _print_test_list("Failed tests", counts.failed)
    if counts.skipped:
        _print_test_list("Skipped tests (not applicable for this backend)", counts.skipped)

    return 0 if not counts.failed else 1
