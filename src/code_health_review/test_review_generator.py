import json
import unittest
from unittest import mock

from fastmcp import FastMCP

from test_utils import mocked_requests_post

from .review_generator import CodeHealthReview


class TestCodeHealthReview(unittest.TestCase):
    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_calculate_code_health_review_json(self, mock_post):
        def mock_analyze_code(file_path: str):
            return json.dumps({"review": ["a", "b", "c"]})

        self.instance = CodeHealthReview(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_review("test.tsx")

        self.assertEqual(json.dumps({"review": ["a", "b", "c"]}), result)

    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_calculate_code_health_review_str(self, mock_post):
        def mock_analyze_code(file_path: str):
            return "string output"

        self.instance = CodeHealthReview(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_review("test.tsx")

        self.assertEqual("string output", result)
