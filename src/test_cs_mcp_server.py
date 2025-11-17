import unittest
import json
import os
from contextlib import asynccontextmanager
from fastmcp import Client
from fastmcp.client.transports import StdioTransport

def cs_access_token():
    """ The MCP server runs in an isolated environment => communicate the 
        token needed to access the CS CLI tool."""
    token = os.getenv("CS_ACCESS_TOKEN")
    if not token:
        raise EnvironmentError("Missing CS_ACCESS_TOKEN environment variable")
    return token

@asynccontextmanager
async def mcp_client():
    use_stdio_for_transport = StdioTransport(
        command="python",
        args=["./src/cs_mcp_server.py"],
        env={"CS_ACCESS_TOKEN": cs_access_token(),
             "CS_MCP_RUNS_TEST_CONTEXT": "True",
             "CS_CLI_PATH": "cs"}) # ensure 'cs' is exposed for the MCP
    async with Client(use_stdio_for_transport) as client:
        yield client

class TestCodeSceneMCP(unittest.IsolatedAsyncioTestCase):
    @classmethod
    def setUpClass(cls):
        cls.file_to_review = "./src/test_data/OrderProcessor.java"

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
                "file_path": self.file_to_review
                })
            code_health_response = result.data
            self.assertIn('Code Health score: 8.65', code_health_response)

    async def test_code_health_review(self):
        async with mcp_client() as c:
            result = await c.call_tool("code_health_review", {
                "file_path": self.file_to_review
                })
            review = json.loads(result.data)
            
            self.assertEqual(8.65, review['score'])
            # Just do a quick sanity check of the review findings:
            self.assertGreater(len(review["review"]), 0, "Expected at least one review item")
            a_review_finding = review["review"][0]
            self.assertEqual(a_review_finding["category"], 'Bumpy Road Ahead')

    async def test_code_health_how_it_works(self):
        async with mcp_client() as c:
            result = await c.call_tool("explain_how_code_health_works")
            self.assertTrue(
                result.data.startswith("# Code Health: how it works"),
                f"URI for the docs is exposed via a tool. Got: {result.data[:50]!r}"
            )

    async def test_code_health_documentation_resources(self):
        async with mcp_client() as c:
            resources = await c.list_resources()
            self.assertTrue(resources, "At least one resource")
            
            code_health_resource = resources[0]
            self.assertTrue(
                str(code_health_resource.uri).startswith("file://codescene-docs/code-health/"),
                f"URI for the docs should be synthetic -- we don't want an absolute path inside our packaging. Got: {code_health_resource.uri}"    
            )
            
            await self.assert_code_health_content(c, code_health_resource)
    
    async def assert_code_health_content(self, connected_client, code_health_resource):
        content = await connected_client.read_resource(code_health_resource.uri)
        
        self.assertTrue(
            content[0].text.startswith("# Code Health: how it works"),
            f"Code Health doc did not start with expected header. Got: {content[:50]!r}"
        )

if __name__ == "__main__":
    unittest.main()
