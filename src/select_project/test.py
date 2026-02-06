import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from test_utils import mocked_requests_post

from . import SelectProject


class TestSelectProject(unittest.TestCase):
    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_select_project_none_found(self, mock_post):
        def mocked_query_api_list_fn(*kwargs):
            return []

        self.instance = SelectProject(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list_fn})

        result = self.instance.select_project()

        self.assertEqual(
            {"projects": [], "link": "https://codescene.io/projects"},
            json.loads(result),
        )

    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_select_project_some_found(self, mock_post):
        def mocked_query_api_list_fn(*kwargs):
            return [{"name": "some project"}]

        self.instance = SelectProject(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list_fn})

        expected = {
            "projects": [{"name": "some project"}],
            "link": "https://codescene.io/projects",
        }

        result = self.instance.select_project()

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_ONPREM_URL": "http://onprem.codescene.local"})
    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_select_project_some_found_onprem(self, mock_post):
        def mocked_query_api_list_fn(*kwargs):
            return [{"name": "some project"}]

        self.instance = SelectProject(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list_fn})

        expected = {
            "projects": [{"name": "some project"}],
            "link": "http://onprem.codescene.local",
        }

        result = self.instance.select_project()

        self.assertEqual(expected, json.loads(result))

    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_select_project_throws(self, mock_post):
        def mocked_query_api_list_fn(*kwargs):
            raise Exception("Some error")

        self.instance = SelectProject(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list_fn})

        result = self.instance.select_project()

        self.assertEqual("Error: Some error", result)

    @mock.patch.dict(os.environ, {"CS_DEFAULT_PROJECT_ID": "1"})
    @mock.patch("requests.post", side_effect=mocked_requests_post)
    def test_env_overwrites_project_id(self, mock_post):
        def mocked_query_api_list_fn(*kwargs):
            return []

        self.instance = SelectProject(FastMCP("Test"), {"query_api_list_fn": mocked_query_api_list_fn})

        expected = {
            "description": "Using default project from CS_DEFAULT_PROJECT_ID environment variable. If you want to be able to select a different project, unset this variable.",
            "id": 1,
            "name": "Default Project (from CS_DEFAULT_PROJECT_ID env var)",
            "link": "https://codescene.io/projects",
        }

        result = self.instance.select_project()

        self.assertEqual(expected, json.loads(result))
