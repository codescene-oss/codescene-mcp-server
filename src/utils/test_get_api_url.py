import os
import unittest
from unittest import mock
from .codescene_api_client import get_api_url


class TestGetApiUrl(unittest.TestCase):
    @mock.patch.dict(os.environ, {"CS_ONPREM_URL": "http://localhost:3003"})
    def test_get_api_url_onprem(self):
        self.assertEqual("http://localhost:3003/api", get_api_url())

    def test_get_api_url_cloud(self):
        self.assertEqual("https://api.codescene.io", get_api_url())
