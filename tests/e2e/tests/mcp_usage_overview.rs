use super::*;

const TIMEOUT: Duration = Duration::from_secs(30);
const RESOURCE_URI: &str = "ui://codescene/mcp-usage-overview";

pub fn test_usage_tool_and_resource() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let tools = client
        .send_request("tools/list", json!({}), TIMEOUT)
        .expect("tools/list should succeed");
    let usage_tool = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "show_mcp_usage_overview")
        .expect("usage tool should be listed");
    assert_eq!(usage_tool["_meta"]["ui"]["resourceUri"], RESOURCE_URI);

    let resource = client
        .send_request("resources/read", json!({ "uri": RESOURCE_URI }), TIMEOUT)
        .expect("usage resource should be readable");
    assert_eq!(
        resource["result"]["contents"][0]["mimeType"],
        "text/html;profile=mcp-app"
    );
    assert!(resource["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .contains("CodeScene MCP usage"));
}
