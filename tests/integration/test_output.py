#!/usr/bin/env python3
"""
Test output formatting utilities.

This module provides functions for formatted test output including
headers, test results, summaries, and boxed module output.
"""

import io
import os
import re
import sys
from contextlib import contextmanager
from dataclasses import dataclass

# ANSI color codes
COLOR_GREEN = "\033[92m"
COLOR_RED = "\033[91m"
COLOR_YELLOW = "\033[93m"
COLOR_RESET = "\033[0m"

# ANSI escape sequence pattern (used to compute visible line width)
_ANSI_ESCAPE_RE = re.compile(r"\033\[[0-9;]*m")

# Box-drawing characters (Unicode vs ASCII fallback)
_BOX_UNICODE = {"tl": "\u250c", "tr": "\u2510", "bl": "\u2514", "br": "\u2518", "h": "\u2500", "v": "\u2502"}
_BOX_ASCII = {"tl": "+", "tr": "+", "bl": "+", "br": "+", "h": "-", "v": "|"}


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
    """Print a formatted test section header.

    The separator line adapts to the terminal width (matching boxed output).
    """
    width = _get_box_width() - 2  # match the interior of print_boxed
    print(f"\n{'=' * width}")
    print(f"  {msg}")
    print(f"{'=' * width}\n")


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


def print_summary(results: list[tuple[str, bool | str]], title: str = "Test Summary") -> int:
    """
    Print test summary and return exit code.

    Args:
        results: List of (test_name, result) tuples where result is:
                 - True: test passed
                 - False: test failed
                 - "SKIPPED": test was skipped
        title: Header text for the summary block.

    Returns:
        0 if all non-skipped tests passed, 1 otherwise
    """
    print_header(title)

    counts = TestCounts.from_results(results)
    _print_counts(counts)

    if counts.failed:
        _print_test_list("Failed tests", counts.failed)
    if counts.skipped:
        _print_test_list("Skipped tests (not applicable for this backend)", counts.skipped)

    return 0 if not counts.failed else 1


# ---------------------------------------------------------------------------
# Boxed output
# ---------------------------------------------------------------------------


def _visible_length(text: str) -> int:
    """Return the visible character count of *text*, ignoring ANSI escapes."""
    return len(_ANSI_ESCAPE_RE.sub("", text))


def _get_box_chars() -> dict[str, str]:
    """Return box-drawing characters appropriate for the current encoding."""
    return _BOX_ASCII if _needs_ascii_fallback() else _BOX_UNICODE


def _get_box_width() -> int:
    """Return the box width based on the terminal, clamped to a sane range."""
    try:
        columns = os.get_terminal_size().columns
    except OSError:
        columns = 80
    return max(40, min(columns, 200))


def _wrap_line(text: str, max_visible: int) -> list[str]:
    """Break *text* into chunks whose visible width fits within *max_visible*.

    ANSI escape sequences are kept attached to the chunk they precede so
    colours survive the split.  Continuation lines are indented by two spaces.
    """
    if _visible_length(text) <= max_visible:
        return [text]

    chunks: list[str] = []
    current = ""
    visible = 0
    continuation_indent = "  "
    limit = max_visible

    i = 0
    while i < len(text):
        # Consume a full ANSI escape without counting visible chars
        if text[i] == "\033":
            end = text.find("m", i)
            if end != -1:
                current += text[i : end + 1]
                i = end + 1
                continue

        if visible >= limit:
            chunks.append(current)
            current = continuation_indent
            visible = _visible_length(continuation_indent)
            limit = max_visible  # subsequent lines use full width

        current += text[i]
        visible += 1
        i += 1

    if current:
        chunks.append(current)

    return chunks


def _format_boxed_line(text: str, inner: int, v: str) -> str:
    """Pad a single line so it fills *inner* visible columns between borders."""
    pad = max(0, inner - _visible_length(text))
    return f"{v}{text}{' ' * pad}{v}"


def print_boxed(lines: list[str], title: str) -> None:
    """Print *lines* inside a bordered box with *title* in the top border.

    Long lines are wrapped onto continuation lines (indented by two spaces)
    so the right border stays aligned.  The box width adapts to the terminal.
    """
    box = _get_box_chars()
    box_width = _get_box_width()
    inner = box_width - 2  # space between the two vertical borders

    title_text = f" {title} "
    fill = max(0, inner - len(title_text) - 1)
    top = f"{box['tl']}{box['h']}{title_text}{box['h'] * fill}{box['tr']}"
    bottom = f"{box['bl']}{box['h'] * inner}{box['br']}"

    print(f"\n{top}")

    for raw_line in lines:
        for chunk in _wrap_line(raw_line.rstrip(), inner):
            print(_format_boxed_line(chunk, inner, box["v"]))

    print(f"{bottom}\n")


@contextmanager
def capture_stdout():
    """Context manager that captures all stdout and yields a list of lines.

    Usage::

        with capture_stdout() as lines:
            print("hello")
        # lines == ["hello"]
    """
    buf = io.StringIO()
    old_stdout = sys.stdout
    sys.stdout = buf
    output: list[str] = []
    try:
        yield output
    finally:
        sys.stdout = old_stdout
        output.extend(buf.getvalue().splitlines())
