import os
import unittest
from unittest import mock
from .codescene_api_client import get_api_request_headers


class TestGetApiRequestHeaders(unittest.TestCase):
    @mock.patch.dict(os.environ, {"CS_ACCESS_TOKEN": "some-token"})
    def test_get_api_request_headers(self):
        self.assertEqual({'Authorization': 'Bearer some-token'}, get_api_request_headers())

    @mock.patch.dict(os.environ, {}, clear=True)
    def test_get_api_request_headers_no_token(self):
        self.assertEqual({}, get_api_request_headers())
