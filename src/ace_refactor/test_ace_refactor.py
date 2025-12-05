import json
import os
import unittest
from unittest import mock

from fastmcp import FastMCP

from . import AceRefactor

class TestAceRefactor(unittest.TestCase):
    def test_refactor(self):
        def mocked_refactoring(*kwargs):
            return {
              "code": "public void start() {}",
              "metadata": {
              },
              "confidence": {
                "description": "high-confidence",
              },
              "reasons": [
                {
                  "summary": "The Large Method code smell remains, but the overall code health improves."
                }
              ],
              "refactoring-properties": {
                "added-code-smells": [
                  "Large Method",
                  "Bumpy Road Ahead",
                  "Complex Method"
                ],
                "removed-code-smells": []
              },
              "reasons-with-details": [
                {
                  "summary": "The Large Method code smell remains, but the overall code health improves."
                }
              ]
          }

        self.instance = AceRefactor(FastMCP("Test"), {
            'post_refactor_fn': mocked_refactoring
        })

        expected = {
            "code": "public void start() {}",
            "declarations": "",
            "confidence": "high-confidence",
            "reasons": ["The Large Method code smell remains, but the overall code health improves."]
        }

        result = self.instance.refactor_code_snippet({
          "file-type": "java",
          "function-type": "StandaloneFn",
          "body": "..."
          }, {
          "category": "Large Method",
          "start-line": 0
        })

        self.assertEqual(expected, json.loads(result))

