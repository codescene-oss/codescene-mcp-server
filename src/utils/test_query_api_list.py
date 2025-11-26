import json
import os
import unittest
from unittest import mock
from utils import get_api_url, query_api_list

def mocked_requests_get(*args, **kwargs):
    class MockResponse:
        def __init__(self, json_data, status_code):
            self.json_data = json_data
            self.status_code = status_code

        def get(self, k):
            return self.json_data[k]

        def json(self):
            return json.loads(self.json_data)

    if args[0] == 'https://api.codescene.io/invalid-data':
        response = {
            'key1': 'key2'
        }

        return MockResponse(json.dumps(response), 200)

    elif args[0] == 'https://api.codescene.io/one-page':
        response = {
            'page': 1,
            'max_pages': 1,
            'items': [
                {'name': 'test'}
            ]
        }
        return MockResponse(json.dumps(response), 200)

    elif args[0] == 'https://api.codescene.io/multiple-pages':
        response = {
            'page': kwargs['params'].get('page', 1),
            'max_pages': 2,
            'items': [
                {'name': 'test'}
            ]
        }
        return MockResponse(json.dumps(response), 200)

    return MockResponse(None, 404)

class TestQueryApiList(unittest.TestCase):
    @mock.patch('requests.get', side_effect=mocked_requests_get)
    def test_query_api_list_invalid_data(self, mock_get):
        result = query_api_list("invalid-data", {}, "key")
        self.assertEqual([], result)

    @mock.patch('requests.get', side_effect=mocked_requests_get)
    def test_query_api_list_one_page(self, mock_get):
        result = query_api_list("one-page", {}, "items")
        self.assertEqual([{'name': 'test'}], result)

    @mock.patch('requests.get', side_effect=mocked_requests_get)
    def test_query_api_list_one_page_invalid_key(self, mock_get):
        result = query_api_list("one-page", {}, "itms")
        self.assertEqual([], result)

    @mock.patch('requests.get', side_effect=mocked_requests_get)
    def test_query_api_list_multiple_pages(self, _):
        result = query_api_list("multiple-pages", {}, "items")
        self.assertEqual([{'name': 'test'}, {'name': 'test'}], result)