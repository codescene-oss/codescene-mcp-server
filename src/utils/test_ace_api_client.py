import json
import unittest
from unittest import mock

import requests

from .ace_api_client import HttpClientError, get_api_endpoint, post_refactor


def mock_requests(endpoint, status_code, content):
    def f(*args, **kwargs):
        response = requests.Response()
        response.headers["content-type"] = "application/json"

        if args[0] == get_api_endpoint(endpoint):
            response.status_code = status_code
            response._content = str.encode(json.dumps(content))
        else:
            response.status_code = 404
            response._content = str.encode(json.dumps({"error": f"{args[0]} {get_api_endpoint(endpoint)}"}))

        return response

    return f


ok_result = {"result": "ok"}


class TestPostRefactor(unittest.TestCase):
    @mock.patch("requests.post", side_effect=mock_requests("api/refactor", 200, ok_result))
    def test_post_refactor_ok(self, mock_post):
        result = post_refactor({})
        self.assertEqual(ok_result, result)
        self.assertEqual(mock_post.call_count, 1)

    @mock.patch(
        "requests.post",
        side_effect=mock_requests("api/refactor", 403, {"error": "some-error"}),
    )
    def test_post_refactor_forward_error_msg_on_403(self, mock_post):
        with self.assertRaises(HttpClientError) as e:
            post_refactor({})

        self.assertEqual("HttpClientError 403: some-error", f"{e.exception}")
        self.assertEqual(mock_post.call_count, 1)

    @mock.patch("requests.post", side_effect=mock_requests("api/refactor", 408, {}))
    def test_post_refactor_retries_on_timeout(self, mock_post):
        with self.assertRaises(HttpClientError):
            post_refactor({})
        self.assertEqual(mock_post.call_count, 3)

    @mock.patch("requests.post", side_effect=mock_requests("api/refactor", 500, {}))
    def test_post_refactor_http_error_on_500(self, mock_post):
        with self.assertRaises(requests.exceptions.HTTPError):
            post_refactor({})
        self.assertEqual(mock_post.call_count, 1)
