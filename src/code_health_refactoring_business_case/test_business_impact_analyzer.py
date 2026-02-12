import json
import unittest

from fastmcp import FastMCP

from errors import CodeSceneCliError

from .business_impact_analyzer import CodeHealthRefactoringBusinessCase


class TestCodeHealthRefactoringBusinessCase(unittest.TestCase):
    def test_code_health_refactoring_business_case(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({"score": 1})

        self.instance = CodeHealthRefactoringBusinessCase(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        result = self.instance.code_health_refactoring_business_case("test.tsx")

        self.assertTrue(json.loads(result)["outcome"]["title"].startswith("Business"))

    def test_code_health_refactoring_business_case_no_score(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({})

        self.instance = CodeHealthRefactoringBusinessCase(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})

        with self.assertRaises(CodeSceneCliError):
            self.instance.code_health_refactoring_business_case("test.tsx")
