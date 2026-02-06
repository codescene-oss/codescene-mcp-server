import json
import unittest

from fastmcp import FastMCP

from .score_calculator import CodeHealthScore


class TestCodeHealthScore(unittest.TestCase):
    def test_calculate_code_health_score_some(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({"score": 1})

        self.instance = CodeHealthScore(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_score("test.tsx")

        self.assertEqual("Code Health score: 1", result)

    def test_calculate_code_health_score_none(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({})

        self.instance = CodeHealthScore(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_score("test.tsx")

        self.assertEqual(
            "Code Health score: Error: CLI output does not contain a 'score' field: {}",
            result,
        )
