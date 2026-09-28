//! Agent instructions setup hint integration tests.
//!
//! Validates that repository-local instruction detection decorates MCP tool
//! responses and that existing CodeScene guidance or configuration suppresses it.

use super::fake_http_server::FakeHttpServer;
use super::*;
use std::time::Instant;

const TOOL_NAME: &str = "code_health_review";
const TEST_FILE: &str = "src/utils/calculator.py";
const TIMEOUT: Duration = Duration::from_secs(60);

fn review_result(client: &mut MCPClient, repo_dir: &Path) -> serde_json::Value {
    let file_path = repo_dir.join(TEST_FILE);
    let response = client
        .call_tool(
            TOOL_NAME,
            json!({"file_path": file_path.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Code Health review tool call should succeed");
    let content = response["result"]["content"]
        .as_array()
        .expect("Tool result should contain content");
    assert_eq!(content.len(), 1, "Hint must not add a content block");
    let text = content
        .first()
        .expect("Tool result should contain primary content")
        .get("text")
        .expect("Primary content should have text")
        .as_str()
        .expect("Primary content should be text");
    serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("Review result should be JSON: {error}: {text}"))
}

fn setup_hint(client: &mut MCPClient, repo_dir: &Path) -> Option<serde_json::Value> {
    review_result(client, repo_dir)
        .get("codescene_setup_hint")
        .cloned()
}

fn start_client(command: &[String], env: &[(String, String)], repo_dir: &Path) -> MCPClient {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

#[test]
fn test_setup_hint_names_existing_instructions_file() {
    let (command, env, repo_dir, _tmp) = setup();
    std::fs::write(repo_dir.join("AGENTS.md"), "Run the test suite.")
        .expect("write generic agent instructions");
    let secondary_rules = repo_dir.join(".amazonq/rules");
    std::fs::create_dir_all(&secondary_rules).expect("create secondary rules directory");
    std::fs::write(
        secondary_rules.join("codescene.md"),
        "Use CodeScene MCP tools.",
    )
    .expect("write secondary CodeScene instructions");
    let mut client = start_client(&command, &env, &repo_dir);

    let result = review_result(&mut client, &repo_dir);
    assert!(result.get("score").is_some(), "Review score should be preserved");
    assert!(result.get("review").is_some(), "Review findings should be preserved");
    let hint = result
        .get("codescene_setup_hint")
        .cloned()
        .expect("Every eligible JSON response should include a hint");
    assert_eq!(hint["variant_id"], "repository-file-without-codescene-v1");
    assert!(hint["message"].as_str().unwrap().contains("AGENTS.md"));
    assert!(hint["message"]
        .as_str()
        .unwrap()
        .contains("Only edit the file if the user says yes"));
    assert!(hint["suggested_instructions"]
        .as_str()
        .unwrap()
        .contains("pre_commit_code_health_safeguard"));
}

#[test]
fn test_setup_hint_skipped_when_codescene_guidance_exists() {
    let (command, env, repo_dir, _tmp) = setup();
    std::fs::write(repo_dir.join("AGENTS.md"), "Use CodeScene MCP tools.")
        .expect("write CodeScene agent instructions");
    let mut client = start_client(&command, &env, &repo_dir);

    assert!(
        setup_hint(&mut client, &repo_dir).is_none(),
        "Existing CodeScene guidance should suppress hints"
    );
}

#[test]
fn test_setup_hint_can_be_disabled() {
    let (command, mut env, repo_dir, _tmp) = setup();
    std::fs::write(repo_dir.join("CLAUDE.md"), "Run the test suite.")
        .expect("write generic agent instructions");
    env.push(("CS_DISABLE_SETUP_HINT".to_string(), "1".to_string()));
    let mut client = start_client(&command, &env, &repo_dir);

    assert!(
        setup_hint(&mut client, &repo_dir).is_none(),
        "CS_DISABLE_SETUP_HINT should suppress hints"
    );
}

#[test]
fn test_setup_hint_can_be_disabled_with_config_tool() {
    let (command, mut env, repo_dir, _tmp) = setup();
    std::fs::write(repo_dir.join("AGENTS.md"), "Run the test suite.")
        .expect("write generic agent instructions");
    use_isolated_config_dir(&mut env, &repo_dir, ".cs_config_setup_hint");
    let mut client = start_client(&command, &env, &repo_dir);
    client
        .call_tool(
            "set_config",
            json!({"key": "disable_setup_hint", "value": "true"}),
            TIMEOUT,
        )
        .expect("disable_setup_hint should be configurable");

    assert!(
        setup_hint(&mut client, &repo_dir).is_none(),
        "disable_setup_hint should suppress hints"
    );
}

#[test]
fn test_setup_hint_sends_analytics_variant() {
    let (command, mut env, repo_dir, _tmp) = setup();
    std::fs::write(repo_dir.join("AGENTS.md"), "Run the test suite.")
        .expect("write generic agent instructions");
    let server = FakeHttpServer::always_ok();
    env.retain(|(key, _)| key != "CS_DISABLE_TRACKING");
    env.push(("CS_TRACKING_URL".to_string(), server.url()));
    let mut client = start_client(&command, &env, &repo_dir);

    setup_hint(&mut client, &repo_dir).expect("A setup hint should be emitted");

    let deadline = Instant::now() + Duration::from_secs(30);
    let event = loop {
        if let Some(event) = server
            .get_payloads()
            .into_iter()
            .find(|payload| payload["event-type"] == "mcp-codescene-setup-hint")
        {
            break event;
        }
        assert!(
            Instant::now() < deadline,
            "Setup hint event was not received"
        );
        std::thread::sleep(Duration::from_millis(100));
    };

    assert_eq!(
        event["event-properties"]["variant-id"],
        "repository-file-without-codescene-v1"
    );
    assert_eq!(event["event-properties"]["message-wording"], "v1");
}
