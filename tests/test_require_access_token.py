"""Unit tests for the require_access_token decorator."""

import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

# Allow importing from the src tree
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from utils.require_access_token import _TOKEN_MISSING_MESSAGE, require_access_token


class TestRequireAccessToken(unittest.TestCase):
    """Tests for the ``require_access_token`` decorator."""

    # -- standalone function --------------------------------------------------

    def test_blocks_when_token_missing(self):
        """Should return the token-missing message when CS_ACCESS_TOKEN is unset."""

        @require_access_token
        def my_tool():
            return "ok"

        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(my_tool(), _TOKEN_MISSING_MESSAGE)

    def test_passes_through_when_token_set(self):
        """Should call the wrapped function normally when the token is present."""

        @require_access_token
        def my_tool():
            return "ok"

        with patch.dict(os.environ, {"CS_ACCESS_TOKEN": "some-token"}):
            self.assertEqual(my_tool(), "ok")

    def test_blocks_when_token_empty_string(self):
        """An empty string should be treated the same as missing."""

        @require_access_token
        def my_tool():
            return "ok"

        with patch.dict(os.environ, {"CS_ACCESS_TOKEN": ""}):
            self.assertEqual(my_tool(), _TOKEN_MISSING_MESSAGE)

    # -- class method (simulates the real tool pattern) -----------------------

    def test_works_with_class_method(self):
        """The decorator must work when applied to a class method receiving self."""

        class FakeTool:
            @require_access_token
            def do_work(self, file_path: str) -> str:
                return f"analyzed {file_path}"

        tool = FakeTool()

        with patch.dict(os.environ, {"CS_ACCESS_TOKEN": "tok"}):
            self.assertEqual(tool.do_work("/tmp/foo.py"), "analyzed /tmp/foo.py")

        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(tool.do_work("/tmp/foo.py"), _TOKEN_MISSING_MESSAGE)

    # -- preserves metadata ---------------------------------------------------

    def test_preserves_docstring(self):
        """functools.wraps should keep the original docstring (FastMCP reads it)."""

        @require_access_token
        def my_tool():
            """Original docstring."""
            return "ok"

        self.assertEqual(my_tool.__doc__, "Original docstring.")

    def test_preserves_function_name(self):
        """functools.wraps should keep the original function name."""

        @require_access_token
        def my_tool():
            return "ok"

        self.assertEqual(my_tool.__name__, "my_tool")

    # -- token-missing message content ----------------------------------------

    def test_message_mentions_set_config(self):
        """The error message should tell users about the set_config tool."""
        self.assertIn("set_config", _TOKEN_MISSING_MESSAGE)

    def test_message_mentions_env_var(self):
        """The error message should mention the CS_ACCESS_TOKEN env var."""
        self.assertIn("CS_ACCESS_TOKEN", _TOKEN_MISSING_MESSAGE)

    def test_message_includes_docs_link(self):
        """The error message should link to the getting-a-personal-access-token docs."""
        self.assertIn("getting-a-personal-access-token", _TOKEN_MISSING_MESSAGE)

    # -- does NOT fire inner decorators when blocked --------------------------

    def test_inner_function_not_called_when_blocked(self):
        """When the token is missing, the wrapped function must NOT execute."""
        call_count = 0

        @require_access_token
        def my_tool():
            nonlocal call_count
            call_count += 1
            return "ok"

        with patch.dict(os.environ, {}, clear=True):
            my_tool()

        self.assertEqual(call_count, 0)


if __name__ == "__main__":
    unittest.main()
