//! Skill resources integration tests.
//!
//! Validates that the MCP server exposes embedded skills as MCP resources
//! using the `skill://` URI scheme, and that the skill management tools
//! (`list_skills`, `get_skill_manifest`, `download_skill`, `sync_skills`)
//! work correctly.

use super::*;

const TIMEOUT: Duration = Duration::from_secs(30);

const EXPECTED_SKILL_NAMES: &[&str] = &[
    "configuring-codescene-mcp",
    "explaining-code-health",
    "guiding-refactoring-with-code-health",
    "installing-and-activating-codescene-mcp",
    "making-the-business-case-for-code-health",
    "prioritizing-technical-debt",
    "risk-based-testing-with-code-health",
    "routing-work-with-code-ownership",
    "safeguarding-ai-generated-code",
];

const TEST_SKILL: &str = "safeguarding-ai-generated-code";

fn extract_tool_text(response: &serde_json::Value) -> Option<String> {
    response
        .get("result")?
        .get("content")?
        .as_array()?
        .first()?
        .get("text")?
        .as_str()
        .map(String::from)
}

fn start_and_initialize(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) -> MCPClient {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

pub fn test_init_capabilities() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");

    let response = client.initialize().expect("Initialize should succeed");
    let capabilities = &response["result"]["capabilities"];

    assert!(
        capabilities.get("resources").is_some(),
        "Initialize should advertise resources capability"
    );
}

pub fn test_list_resources() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let response = client
        .send_request("resources/list", json!({}), TIMEOUT)
        .expect("resources/list should succeed");

    let resources = response["result"]["resources"]
        .as_array()
        .expect("resources should be an array");

    let expected_count = EXPECTED_SKILL_NAMES.len() * 2;
    assert_eq!(
        resources.len(),
        expected_count,
        "Expected {expected_count} resources, got {}",
        resources.len()
    );

    let uris: Vec<&str> = resources
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .collect();

    for name in EXPECTED_SKILL_NAMES {
        let skill_uri = format!("skill://{name}/SKILL.md");
        assert!(uris.contains(&skill_uri.as_str()), "Missing {skill_uri}");

        let manifest_uri = format!("skill://{name}/_manifest");
        assert!(
            uris.contains(&manifest_uri.as_str()),
            "Missing {manifest_uri}"
        );
    }
}

pub fn test_read_skill_md() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let uri = format!("skill://{TEST_SKILL}/SKILL.md");
    let response = client
        .send_request("resources/read", json!({"uri": uri}), TIMEOUT)
        .expect("resources/read should succeed");

    let contents = response["result"]["contents"]
        .as_array()
        .expect("contents should be an array");

    assert!(!contents.is_empty(), "Response should have contents");

    let text = contents[0]["text"]
        .as_str()
        .expect("First content should have text");

    assert!(
        text.len() > 50,
        "Content should be > 50 chars, got {}",
        text.len()
    );
    assert!(
        text.contains("---"),
        "Content should contain frontmatter delimiter"
    );
}

pub fn test_read_manifest() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let uri = format!("skill://{TEST_SKILL}/_manifest");
    let response = client
        .send_request("resources/read", json!({"uri": uri}), TIMEOUT)
        .expect("resources/read should succeed");

    let contents = response["result"]["contents"]
        .as_array()
        .expect("contents should be an array");

    assert!(!contents.is_empty(), "Response should have contents");

    let text = contents[0]["text"]
        .as_str()
        .expect("First content should have text");

    let manifest: serde_json::Value =
        serde_json::from_str(text).expect("Manifest should be valid JSON");

    assert_eq!(
        manifest["skill"].as_str(),
        Some(TEST_SKILL),
        "Manifest should have correct skill name"
    );

    let files = manifest["files"]
        .as_array()
        .expect("Manifest should have files array");

    assert_eq!(files.len(), 1, "Manifest should list one file");
}

pub fn test_list_resource_templates() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let response = client
        .send_request("resources/templates/list", json!({}), TIMEOUT)
        .expect("resources/templates/list should succeed");

    let templates = response["result"]["resourceTemplates"]
        .as_array()
        .expect("resourceTemplates should be an array");

    assert!(!templates.is_empty(), "Should return at least one template");

    let has_skill_template = templates.iter().any(|t| {
        t["uriTemplate"]
            .as_str()
            .is_some_and(|u| u.contains("skill://"))
    });

    assert!(has_skill_template, "Templates should contain skill:// URI");
}

pub fn test_read_error_cases() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let error_uris = [
        "skill://nonexistent-skill/SKILL.md",
        "file:///etc/passwd",
    ];

    for uri in error_uris {
        let response = client
            .send_request("resources/read", json!({"uri": uri}), TIMEOUT)
            .expect("Request should not fail at transport level");

        assert!(
            response.get("error").is_some(),
            "Reading '{uri}' should return an error"
        );
    }
}

pub fn test_list_skills_tool() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let response = client
        .call_tool("list_skills", json!({}), TIMEOUT)
        .expect("list_skills should succeed");

    let content = extract_tool_text(&response).expect("Should return text content");
    let expected_header = format!("Available skills ({})", EXPECTED_SKILL_NAMES.len());

    assert!(
        content.contains(&expected_header),
        "Should list correct skill count"
    );
    assert!(
        content.contains(TEST_SKILL),
        "Should contain {TEST_SKILL}"
    );
}

pub fn test_get_skill_manifest_tool() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let response = client
        .call_tool(
            "get_skill_manifest",
            json!({"skill_name": TEST_SKILL}),
            TIMEOUT,
        )
        .expect("get_skill_manifest should succeed");

    let content = extract_tool_text(&response).expect("Should return text content");
    let manifest: serde_json::Value =
        serde_json::from_str(&content).expect("Content should be valid JSON");

    assert_eq!(
        manifest["skill"].as_str(),
        Some(TEST_SKILL),
        "Manifest should have correct skill name"
    );
}

pub fn test_download_skill_tool() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let dest = repo_dir.join("download_test");
    let response = client
        .call_tool(
            "download_skill",
            json!({
                "skill_name": TEST_SKILL,
                "destination_dir": dest.to_string_lossy(),
            }),
            TIMEOUT,
        )
        .expect("download_skill should succeed");

    let content = extract_tool_text(&response).expect("Should return text content");
    assert!(content.contains("Downloaded"), "Should report success");

    let skill_file = dest.join(TEST_SKILL).join("SKILL.md");
    assert!(
        skill_file.exists(),
        "SKILL.md should be written to disk at {}",
        skill_file.display()
    );
}

pub fn test_sync_skills_tool() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = start_and_initialize(&command, &env, &repo_dir);

    let dest = repo_dir.join("sync_test");
    let response = client
        .call_tool(
            "sync_skills",
            json!({"destination_dir": dest.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("sync_skills should succeed");

    let content = extract_tool_text(&response).expect("Should return text content");
    assert!(content.contains("Downloaded"), "Should report success");

    let synced_dirs: Vec<_> = std::fs::read_dir(&dest)
        .expect("Destination should exist")
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .collect();

    assert_eq!(
        synced_dirs.len(),
        EXPECTED_SKILL_NAMES.len(),
        "All {} skills should be synced, got {}",
        EXPECTED_SKILL_NAMES.len(),
        synced_dirs.len()
    );
}
