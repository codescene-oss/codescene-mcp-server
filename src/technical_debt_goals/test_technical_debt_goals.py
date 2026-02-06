import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from test_utils import mocked_requests_post

from . import TechnicalDebtGoals

PROJECT_ID = 3
FILE_PATH = "/some-path/some_file.tsx"
FILE_NAME = "some_file.tsx"

# -----------------------------------------------------------------------------
# Test Strategy: Table-Driven Tests
# -----------------------------------------------------------------------------
# This test suite uses a table-driven approach, where each test scenario is
# represented as a dictionary containing:
#   - scenario name
#   - mock API return value
#   - expected output
#   - environment setup (on-prem/cloud, project/file)
#   - error simulation (if applicable)
#
# The main test method loops over all scenarios, using subTest for isolation.
# Each scenario is executed with its own environment and mock, and the result
# is asserted against the expected output.
#
# This strategy ensures:
#   - All key behaviors are covered (none/some/error, project/file, cloud/on-prem)
#   - Adding new scenarios is easy and does not require new test methods
#   - The test logic is concise, maintainable, and intent-revealing
# -----------------------------------------------------------------------------

project_goals_none_found = {
    "name": "project_goals_none_found",
    "mock_return": [],
    "expected": {
        "files": [],
        "description": f"Found 0 files with technical debt goals for project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/biomarkers",
    },
    "onprem": False,
    "method": "project",
}
project_goals_some_found = {
    "name": "project_goals_some_found",
    "mock_return": [{"path": "some_path"}],
    "expected": {
        "files": [{"path": "some_path"}],
        "description": f"Found 1 files with technical debt goals for project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/biomarkers",
    },
    "onprem": False,
    "method": "project",
}
project_goals_some_found_onprem = {
    "name": "project_goals_some_found_onprem",
    "mock_return": [{"path": "some_path"}],
    "expected": {
        "files": [{"path": "some_path"}],
        "description": f"Found 1 files with technical debt goals for project ID {PROJECT_ID}.",
        "link": f"https://onprem-codescene.io/{PROJECT_ID}/analyses/latest/code/biomarkers",
    },
    "onprem": True,
    "method": "project",
}
project_goals_error = {
    "name": "project_goals_error",
    "mock_return": Exception("Some error"),
    "expected": "Error: Some error",
    "onprem": False,
    "method": "project",
    "is_error": True,
}

file_goals_none_found = {
    "name": "file_goals_none_found",
    "mock_return": [],
    "expected": {
        "goals": [],
        "description": f"Found 0 technical debt goals for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/biomarkers?name={FILE_NAME}",
    },
    "onprem": False,
    "method": "file",
}
file_goals_some_found = {
    "name": "file_goals_some_found",
    "mock_return": [{"path": FILE_NAME, "goals": [{"name": "some goal"}]}],
    "expected": {
        "goals": [{"name": "some goal"}],
        "description": f"Found 1 technical debt goals for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/biomarkers?name={FILE_NAME}",
    },
    "onprem": False,
    "method": "file",
}
file_goals_some_found_onprem = {
    "name": "file_goals_some_found_onprem",
    "mock_return": [{"path": FILE_NAME, "goals": [{"name": "some goal"}]}],
    "expected": {
        "goals": [{"name": "some goal"}],
        "description": f"Found 1 technical debt goals for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://onprem-codescene.io/{PROJECT_ID}/analyses/latest/code/biomarkers?name={FILE_NAME}",
    },
    "onprem": True,
    "method": "file",
}
file_goals_error = {
    "name": "file_goals_error",
    "mock_return": Exception("Some error"),
    "expected": "Error: Some error",
    "onprem": False,
    "method": "file",
    "is_error": True,
}


class TestTechnicalDebtGoals(unittest.TestCase):
    def setUp(self):
        self.project_id = PROJECT_ID
        self.file_path = FILE_PATH
        self.file_name = FILE_NAME

    def _patch_environment(self, scenario):
        patch_dict = {}
        if scenario["onprem"]:
            patch_dict["CS_ONPREM_URL"] = "https://onprem-codescene.io"
        if scenario["method"] == "file":
            patch_dict["CS_MOUNT_PATH"] = "/some-path"
        return mock.patch.dict(os.environ, patch_dict)

    def _make_mocked_instance(self, scenario):
        def mocked_query_api_list(*args, **kwargs):
            if scenario.get("is_error"):
                raise scenario["mock_return"]
            return scenario["mock_return"]

        return TechnicalDebtGoals(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list})

    def _execute_scenario(self, scenario):
        with self._patch_environment(scenario), mock.patch("requests.post", side_effect=mocked_requests_post):
            instance = self._make_mocked_instance(scenario)
            if scenario["method"] == "project":
                return instance.list_technical_debt_goals_for_project(self.project_id)

            return instance.list_technical_debt_goals_for_project_file(self.file_path, self.project_id)

    def _assert_scenario(self, scenario):
        result = self._execute_scenario(scenario)
        if isinstance(result, str) and not scenario.get("is_error"):
            result = json.loads(result)
        self.assertEqual(scenario["expected"], result)

    def test_td_goal_scenarios(self):
        """Table-driven test for technical debt goals scenarios."""
        all_goal_scenarios = [
            project_goals_none_found,
            project_goals_some_found,
            project_goals_some_found_onprem,
            project_goals_error,
            file_goals_none_found,
            file_goals_some_found,
            file_goals_some_found_onprem,
            file_goals_error,
        ]
        for scenario in all_goal_scenarios:
            with self.subTest(scenario=scenario["name"]):
                self._assert_scenario(scenario)

    @mock.patch("technical_debt_goals.technical_debt_goals.get_relative_file_path_for_api")
    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_file_goals_static_mode(self, mock_post, mock_get_path):
        """Test that file-level goals work in static executable mode (no CS_MOUNT_PATH)."""
        mock_get_path.return_value = "src/some_file.tsx"

        def mocked_query_api_list(*args, **kwargs):
            return [{"path": "src/some_file.tsx", "goals": [{"name": "reduce complexity"}]}]

        instance = TechnicalDebtGoals(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list})
        result = instance.list_technical_debt_goals_for_project_file("/some/git/repo/src/some_file.tsx", PROJECT_ID)

        mock_get_path.assert_called_once_with("/some/git/repo/src/some_file.tsx")
        result_data = json.loads(result)
        self.assertEqual(result_data["goals"], [{"name": "reduce complexity"}])
        self.assertIn("src/some_file.tsx", result_data["description"])
