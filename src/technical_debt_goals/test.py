import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from . import TechnicalDebtGoals

class TestTechnicalDebtGoals(unittest.TestCase):
    def test_list_technical_debt_goals_for_project_none_found(self):
        def mocked_query_api_list(*kwargs):
            return []

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = {
            "files": [],
            "description": "Found 0 files with technical debt goals for project ID 3.",
            "link": "https://codescene.io/projects/3/analyses/latest/code/biomarkers"
        }

        result = self.instance.list_technical_debt_goals_for_project(3)

        self.assertEqual(expected, json.loads(result))

    def test_list_technical_debt_goals_for_project_some_found(self):
        def mocked_query_api_list(*kwargs):
            return [{
                'path': 'some_path'
            }]

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = {
            "files": [{
                'path': 'some_path'
            }],
            "description": "Found 1 files with technical debt goals for project ID 3.",
            "link": "https://codescene.io/projects/3/analyses/latest/code/biomarkers"
        }

        result = self.instance.list_technical_debt_goals_for_project(3)

        self.assertEqual(expected, json.loads(result))

    def test_list_technical_debt_goals_for_project_throws(self):
        def mocked_query_api_list(*kwargs):
            raise Exception("Some error")

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = "Error: Some error"
        result = self.instance.list_technical_debt_goals_for_project(3)

        self.assertEqual(expected, result)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path"})
    def test_list_technical_debt_goals_for_project_file_none_found(self):
        def mocked_query_api_list(*kwargs):
            return []

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = {
            "goals": [],
            "description": "Found 0 technical debt goals for file some_file.tsx in project ID 3.",
            "link": "https://codescene.io/projects/3/analyses/latest/code/biomarkers?name=some_file.tsx"
        }

        result = self.instance.list_technical_debt_goals_for_project_file("/some-path/some_file.tsx", 3)

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path"})
    def test_list_technical_debt_goals_for_project_file_some_found(self):
        def mocked_query_api_list(*kwargs):
            return [{
                'path': 'some_file.tsx',
                'goals': [{'name': 'some goal'}]
            }]

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = {
            "goals": [{'name': 'some goal'}],
            "description": "Found 1 technical debt goals for file some_file.tsx in project ID 3.",
            "link": "https://codescene.io/projects/3/analyses/latest/code/biomarkers?name=some_file.tsx"
        }

        result = self.instance.list_technical_debt_goals_for_project_file("/some-path/some_file.tsx", 3)

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path", "CS_ONPREM_URL": "http://onprem-codescene"})
    def test_list_technical_debt_goals_for_project_file_some_found_onprem(self):
        def mocked_query_api_list(*kwargs):
            return [{
                'path': 'some_file.tsx',
                'goals': [{'name': 'some goal'}]
            }]

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = {
            "goals": [{'name': 'some goal'}],
            "description": "Found 1 technical debt goals for file some_file.tsx in project ID 3.",
            "link": "http://onprem-codescene/3/analyses/latest/code/biomarkers?name=some_file.tsx"
        }

        result = self.instance.list_technical_debt_goals_for_project_file("/some-path/some_file.tsx", 3)

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path"})
    def test_list_technical_debt_goals_for_project_file_throws(self):
        def mocked_query_api_list(*kwargs):
            raise Exception("Some error")

        self.instance = TechnicalDebtGoals(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = "Error: Some error"
        result = self.instance.list_technical_debt_goals_for_project_file("/some-path/some_file.tsx", 3)

        self.assertEqual(expected, result)
        
