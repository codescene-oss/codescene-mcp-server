import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from . import CodeOwnership

class TestCodeOwnership(unittest.TestCase):
    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path"})
    def test_code_ownership_none_found(self):
        def mocked_query_api_list(*kwargs):
            return []

        self.instance = CodeOwnership(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = []
        result = self.instance.code_ownership_for_path(3, "/some-path/some_file.tsx")

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path"})
    def test_code_ownership_some_found(self):
        def mocked_query_api_list(*kwargs):
            return [{
                'owner': 'some_owner',
                'path': '/some-path/some_file.tsx'
            }, {
                'owner': 'some_owner',
                'path': '/some-path/some_file2.tsx'
            },
            {
                'owner': 'some_owner2',
                'path': '/some-path/some_file3.tsx'
            }]

        self.instance = CodeOwnership(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = [{
            "owner": "some_owner",
            "paths": ["/some-path/some_file.tsx", "/some-path/some_file2.tsx"],
            "link": "https://codescene.io/projects/3/analyses/latest/social/individuals/system-map?author=author:some_owner"
        }, {
            "owner": "some_owner2",
            "paths": ["/some-path/some_file3.tsx"],
            "link": "https://codescene.io/projects/3/analyses/latest/social/individuals/system-map?author=author:some_owner2"
        }]

        result = self.instance.code_ownership_for_path(3, "/some-path/some_file.tsx")

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path", "CS_ONPREM_URL": "https://onprem-codescene.io"})
    def test_code_ownership_some_found_onprem(self):
        def mocked_query_api_list(*kwargs):
            return [{
                'owner': 'some_owner',
                'path': '/some-path/some_file.tsx'
            }]

        self.instance = CodeOwnership(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = [{
            "owner": "some_owner",
            "paths": ["/some-path/some_file.tsx"],
            "link": "https://onprem-codescene.io/3/analyses/latest/social/individuals/system-map?author=author:some_owner"
        }]

        result = self.instance.code_ownership_for_path(3, "/some-path/some_file.tsx")

        self.assertEqual(expected, json.loads(result))

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/some-path"})
    def test_code_ownership_throws(self):
        def mocked_query_api_list(*kwargs):
            raise Exception("Some error")

        self.instance = CodeOwnership(FastMCP("Test"), {
            'query_api_list_fn': mocked_query_api_list
        })

        expected = "Error: Some error"
        result = self.instance.code_ownership_for_path(3, "/some-path/some_file.tsx")

        self.assertEqual(expected, result)
