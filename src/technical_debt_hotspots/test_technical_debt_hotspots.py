import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP
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
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots"
    },
    "onprem": False,
    "method": "project"
}
project_single_hotspot_found = {
    "name": "project_some_found",
    "mock_return": [{"path": "some_path"}],
    "expected": {
        "hotspots": [{"path": "some_path"}],
        "description": f"Found 1 files with technical debt hotspots for project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots"
    },
    "onprem": False,
    "method": "project"
}
project_single_hotspot_found_onprem = {
    "name": "project_some_found_onprem",
    "mock_return": [{"path": "some_path"}],
    "expected": {
        "hotspots": [{"path": "some_path"}],
        "description": f"Found 1 files with technical debt hotspots for project ID {PROJECT_ID}.",
        "link": f"https://onprem-codescene.io/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots"
    },
    "onprem": True,
    "method": "project"
}

# Now provide the variations on hotspots files to test:
file_none_found = {
    "name": "file_none_found",
    "mock_return": [],
    "expected": {
        "hotspot": {},
        "description": f"Found no technical debt hotspot for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots"
    },
    "onprem": False,
    "method": "file"
}
file_single_hotspot_found = {
    "name": "file_some_found",
    "mock_return": [{"path": FILE_NAME, "revisions": 55}],
    "expected": {
        "hotspot": {"path": FILE_NAME, "revisions": 55},
        "description": f"Found technical debt hotspot for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://codescene.io/projects/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots"
    },
    "onprem": False,
    "method": "file"
}
file_single_hotspot_found_onprem = {
    "name": "file_some_found_onprem",
    "mock_return": [{"path": FILE_NAME, "revisions": 55}],
    "expected": {
        "hotspot": {"path": FILE_NAME, "revisions": 55},
        "description": f"Found technical debt hotspot for file {FILE_NAME} in project ID {PROJECT_ID}.",
        "link": f"https://onprem-codescene.io/{PROJECT_ID}/analyses/latest/code/technical-debt/system-map#hotspots"
    },
    "onprem": True,
    "method": "file"
}
class TestTechnicalDebtHotspots(unittest.TestCase):
    def setUp(self):
        self.project_id = PROJECT_ID
        self.file_path = FILE_PATH
        self.file_name = FILE_NAME

    def make_instance(self, query_api_list_fn):
        return TechnicalDebtHotspots(FastMCP("Test"), {'query_api_list_fn': query_api_list_fn})

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
            return scenario["mock_return"]
        return TechnicalDebtHotspots(FastMCP("Test"), {'query_api_list_fn': mocked_query_api_list})

    def _execute_scenario(self, scenario):
        with self._patch_environment(scenario):
            instance = self._make_mocked_instance(scenario)
            if scenario["method"] == "project":
                return instance.list_technical_debt_hotspots_for_project(self.project_id)
          
            return instance.list_technical_debt_hotspots_for_project_file(self.file_path, self.project_id)

    def _assert_scenario(self, scenario):
        result = self._execute_scenario(scenario)
        self.assert_json_result(result, scenario["expected"])

    def test_td_hotspots_api_scenarios(self):
        """A table-driven test that executes all technical debt hotspot scenarios."""
        all_scenarios = [
            project_none_found,
            project_single_hotspot_found,
            project_single_hotspot_found_onprem,
            file_none_found,
            file_single_hotspot_found,
            file_single_hotspot_found_onprem,
        ]
        for scenario in all_scenarios:
            with self.subTest(scenario=scenario["name"]):
                self._assert_scenario(scenario)
