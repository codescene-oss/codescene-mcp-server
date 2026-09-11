//! Integration test for clients that probe server capabilities before initialization.
//!
//! Validates that an unsupported `server/discover` request receives a standard
//! JSON-RPC error without terminating the process, allowing initialization and
//! subsequent tool requests to succeed on the same stdio connection.

use super::*;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn test_server_discover_falls_back_without_exiting() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");

    let discovery = client
        .send_request("server/discover", json!({}), RESPONSE_TIMEOUT)
        .expect("server/discover should receive an error response");
    assert_eq!(discovery["error"]["code"], json!(-32601));

    client
        .initialize()
        .expect("Initialize should succeed after server/discover");
    let tools = client
        .send_request("tools/list", json!({}), RESPONSE_TIMEOUT)
        .expect("Server should remain available after initialization");
    assert!(tools["result"]["tools"].as_array().is_some_and(|tools| !tools.is_empty()));
}
