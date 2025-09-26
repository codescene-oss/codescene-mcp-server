import asyncio
from fastmcp import Client

# In-memory server (ideal for testing)
#server = FastMCP("CodeScene")
#client = Client(server)

# HTTP server
#client = Client("https://example.com/mcp")

# Local Python script
client = Client("./cs_mcp_server.py")

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