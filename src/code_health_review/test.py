import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from .review_generator import CodeHealthReview


@mock.patch.dict(os.environ, {"CS_ACCESS_TOKEN": "test-token"})
class TestCodeHealthReview(unittest.TestCase):
    def test_calculate_code_health_review_json(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({"review": ["a", "b", "c"]})

        self.instance = CodeHealthReview(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_review("test.tsx")

        self.assertEqual(json.dumps({"review": ["a", "b", "c"]}), result)

    def test_calculate_code_health_review_str(self):
        def mock_analyze_code(file_path: str):
            return "string output"

        self.instance = CodeHealthReview(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_review("test.tsx")

        self.assertEqual("string output", result)
