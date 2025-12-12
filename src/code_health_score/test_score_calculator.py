import json
import unittest
from unittest import mock
from fastmcp import FastMCP

from test_utils import mocked_requests_post
from .score_calculator import CodeHealthScore

class TestCodeHealthScore(unittest.TestCase):
    @mock.patch('requests.post', side_effect=mocked_requests_post)
    def test_calculate_code_health_score_some(self, mock_post):
        def mock_analyze_code(file_path: str):
            return json.dumps({'score': 1})

        self.instance = CodeHealthScore(FastMCP("Test"), {
            'analyze_code_fn': mock_analyze_code
        })

        result = self.instance.code_health_score("test.tsx")

        self.assertEqual("Code Health score: 1", result)

    @mock.patch('requests.post', side_effect=mocked_requests_post)
    def test_calculate_code_health_score_none(self, mock_post):
        def mock_analyze_code(file_path: str):
            return json.dumps({})

        self.instance = CodeHealthScore(FastMCP("Test"), {
            'analyze_code_fn': mock_analyze_code
        })

        result = self.instance.code_health_score("test.tsx")

        self.assertEqual("Code Health score: Error: CLI output does not contain a 'score' field: {}", result)