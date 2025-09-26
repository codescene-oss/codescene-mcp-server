import asyncio
from fastmcp import Client
import os

def include_cs_pat_token():
    """ The CS CLI tool requires a PAT token. Since we invoke the CLI from a 
    subprocess, we need to propage that env variable: the MCP server runs 
    in isolated environments -- a security feature.

    (See https://codescene.io/docs/cli/index.html for instructions on the PAT.)
    """
    return {"CS_ACCESS_TOKEN": os.getenv("CS_ACCESS_TOKEN")}

# Local Python script
client = Client("./cs_mcp_server.py")
client.transport.env = include_cs_pat_token() # Is this really the way? The docs seem outdated...

async def main():
    async with client:
        # Basic server interaction
        await client.ping()
        
        # List available operations
        tools = await client.list_tools()
        print("\n==============================\n")
        print(tools)
        #resources = await client.list_resources()
        #prompts = await client.list_prompts()
        
        # Execute operations
        file_to_review = "/Users/adam/Documents/Programming/NetBeansProjects/cacs_product/debug/ace_demo_repos/csharp/PowerToys/src/modules/MouseWithoutBorders/App/Form/frmScreen.cs"
        
        print("\n==============================\n")
        code_health_score = await client.call_tool("code_health_score", {"file_path": file_to_review})
        print(code_health_score)

        print("\n==============================\n")
        code_health_review_result = await client.call_tool("code_health_review", {"file_path": file_to_review})
        print(code_health_review_result)

asyncio.run(main())