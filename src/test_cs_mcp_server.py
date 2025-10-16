import unittest
import json
import os
from contextlib import asynccontextmanager
from fastmcp import Client
from fastmcp.client.transports import StdioTransport

def include_cs_pat_token():
    """ The MCP server runs in an isolated environment => communicate the 
        token needed to access the CS CLI tool."""
    token = os.getenv("CS_ACCESS_TOKEN")
    if not token:
        raise EnvironmentError("Missing CS_ACCESS_TOKEN environment variable")
    return {"CS_ACCESS_TOKEN": token}

@asynccontextmanager
async def mcp_client():
    use_stdio_for_transport = StdioTransport(
        command="python",
        args=["./src/cs_mcp_server.py"],
        env=include_cs_pat_token())
    async with Client(use_stdio_for_transport) as client:
        yield client

class TestCodeSceneMCP(unittest.IsolatedAsyncioTestCase):
    @classmethod
    def setUpClass(cls):
        cls.file_to_review = "./src/test_data/OrderProcessor.java"

        with open(cls.file_to_review, 'r') as f:
            cls.file_content = f.read()
            cls.file_ext = '.java'

    async def test_ping(self):
        async with mcp_client() as c:
            ponged = await c.ping()
            self.assertTrue(ponged)

    async def test_list_tools(self):
        async with mcp_client() as c:
            tools = await c.list_tools()
            tool_names = [n.name for n in tools]
            self.assertEqual(['code_health_score', 'code_health_review'], tool_names)

    async def test_code_health_score(self):
        async with mcp_client() as c:
            result = await c.call_tool("code_health_score", {
                "file_content": self.file_content,
                "file_ext": self.file_ext
            })
            code_health_response = result.data
            self.assertIn('Code Health score: 8.65', code_health_response)

    async def test_code_health_review(self):
        async with mcp_client() as c:
            result = await c.call_tool("code_health_review", {
                "file_content": self.file_content,
                "file_ext": self.file_ext
            })
            review = json.loads(result.data)
            
            self.assertEqual(8.65, review['score'])
            # Just do a quick sanity check of the review findings:
            self.assertGreater(len(review["review"]), 0, "Expected at least one review item")
            a_review_finding = review["review"][0]
            self.assertEqual(a_review_finding["category"], 'Bumpy Road Ahead')

if __name__ == "__main__":
    unittest.main()
