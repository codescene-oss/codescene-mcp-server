import os
import unittest
from unittest import mock
from utils import get_api_url, query_api_list


class TestQueryApiList(unittest.TestCase):
    def test_query_api_list_one_page(self):
        query_api_list("/test", {})
        self.assertEqual([], query_api_list())


if __name__ == '__main__':
    unittest.main()
