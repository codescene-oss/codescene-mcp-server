import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from test_utils import mocked_requests_post

from . import TechnicalDebtHotspots

PROJECT_ID = 3
FILE_PATH = "/some-path/some_file.tsx"
FILE_NAME = "some_file.tsx"

# -----------------------------------------------------------------------------
# Scenario Data Purpose
# -----------------------------------------------------------------------------
# The scenario data below represents all key situations the technical debt
# hotspot API should handle for both project-level and file-level queries.
# Each scenario dictionary specifies:
#   - The test case name ('name')
#   - The mock data returned by the API ('mock_return')
#   - The expected output ('expected')
#   - Whether the test simulates an on-premises CodeScene instance ('onprem')
#   - Whether the query is for a whole project or a specific file ('method')
#
# These scenarios ensure the test suite covers:
#   - No hotspots found (empty results)
#   - A single hotspot found (with details)
#   - Correct handling of both cloud and on-prem URLs
#   - Both project-wide and file-specific queries
#
# This structure makes the tests easy to extend and guarantees robust coverage
# of all major API behaviors.
# -----------------------------------------------------------------------------

project_none_found = {
    "name": "project_none_found",
    "mock_return": [],
    "expected": {
        "hotspots": [],
        "description": f"Found 0 files with technical debt hotspots for project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots",
    },
    "onprem": False,
    "method": "project",
}

# New scenario: project-level error handling
project_error = {
    "name": "project_error",
    "mock_return": Exception("Simulated API error"),
    "expected": "Error: Simulated API error",
    "onprem": False,
    "method": "project",
    "is_error": True,
}
project_single_hotspot_found = {
    "name": "project_some_found",
    "mock_return": [{"path": "some_path"}],
    "expected": {
        "hotspots": [{"path": "some_path"}],
        "description": f"Found 1 files with technical debt hotspots for project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots",
    },
    "onprem": False,
    "method": "project",
}
project_single_hotspot_found_onprem = {
    "name": "project_some_found_onprem",
    "mock_return": [{"path": "some_path"}],
    "expected": {
        "hotspots": [{"path": "some_path"}],
        "description": f"Found 1 files with technical debt hotspots for project ID {PROJECT_ID}.",
        "link": f"https://onprem-codescene.io/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots",
    },
    "onprem": True,
    "method": "project",
}

# Now provide the variations on hotspots files to test:
file_none_found = {
    "name": "file_none_found",
    "mock_return": [],
    "expected": {
        "hotspot": {},
        "description": f"Found no technical debt hotspot for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots",
    },
    "onprem": False,
    "method": "file",
}
file_single_hotspot_found = {
    "name": "file_some_found",
    "mock_return": [{"path": FILE_NAME, "revisions": 55}],
    "expected": {
        "hotspot": {"path": FILE_NAME, "revisions": 55},
        "description": f"Found technical debt hotspot for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots",
    },
    "onprem": False,
    "method": "file",
}
file_single_hotspot_found_onprem = {
    "name": "file_some_found_onprem",
    "mock_return": [{"path": FILE_NAME, "revisions": 55}],
    "expected": {
        "hotspot": {"path": FILE_NAME, "revisions": 55},
        "description": f"Found technical debt hotspot for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://onprem-codescene.io/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots",
    },
    "onprem": True,
    "method": "file",
}

# New scenario: file-level error handling
file_error = {
    "name": "file_error",
    "mock_return": Exception("Simulated file API error"),
    "expected": "Error: Simulated file API error",
    "onprem": False,
    "method": "file",
    "is_error": True,
}


class TestTechnicalDebtHotspots(unittest.TestCase):
    def setUp(self):
        self.project_id = PROJECT_ID
        self.file_path = FILE_PATH
        self.file_name = FILE_NAME

    def make_instance(self, query_api_list_fn):
        return TechnicalDebtHotspots(FastMCP("Test"), {"query_api_list_fn": query_api_list_fn})

    def assert_json_result(self, actual, expected):
        self.assertEqual(expected, json.loads(actual))

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

        return TechnicalDebtHotspots(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list})

    def _execute_scenario(self, scenario):
        with self._patch_environment(scenario), mock.patch("requests.post", side_effect=mocked_requests_post):
            instance = self._make_mocked_instance(scenario)
            if scenario["method"] == "project":
                return instance.list_technical_debt_hotspots_for_project(self.project_id)
            return instance.list_technical_debt_hotspots_for_project_file(self.file_path, self.project_id)

    def _assert_scenario(self, scenario):
        result = self._execute_scenario(scenario)
        if scenario.get("is_error"):
            self.assertEqual(scenario["expected"], result)
        else:
            self.assert_json_result(result, scenario["expected"])

    def test_td_hotspots_api_scenarios(self):
        """
        Table-driven test for technical debt hotspot API scenarios.
        Includes a scenario that simulates an API error for project-level queries
        to ensure error handling code is covered and intent is clear.
        """
        all_scenarios = [
            project_none_found,
            project_single_hotspot_found,
            project_single_hotspot_found_onprem,
            file_none_found,
            file_single_hotspot_found,
            file_single_hotspot_found_onprem,
            project_error,  # Ensures error handling for project-level API is tested
            file_error,  # Ensures error handling for file-level API is tested
        ]
        for scenario in all_scenarios:
            with self.subTest(scenario=scenario["name"]):
                self._assert_scenario(scenario)

    @mock.patch("technical_debt_hotspots.technical_debt_hotspots.get_relative_file_path_for_api")
    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_file_hotspots_static_mode(self, mock_post, mock_get_path):
        """Test that file-level hotspots work in static executable mode (no CS_MOUNT_PATH)."""
        mock_get_path.return_value = "src/some_file.tsx"

        def mocked_query_api_list(*args, **kwargs):
            return [{"path": "src/some_file.tsx", "revisions": 42, "code_health": 5.5}]

        instance = TechnicalDebtHotspots(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list})
        result = instance.list_technical_debt_hotspots_for_project_file("/some/git/repo/src/some_file.tsx", PROJECT_ID)

        mock_get_path.assert_called_once_with("/some/git/repo/src/some_file.tsx")
        result_data = json.loads(result)
        self.assertEqual(result_data["hotspot"]["revisions"], 42)
        self.assertIn("src/some_file.tsx", result_data["description"])
