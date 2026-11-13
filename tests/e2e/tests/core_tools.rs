//! Core tool integration tests.
//!
//! Exercises the primary Code Health tools end to end against the sample
//! repository: scoring, reviews, the pre-commit safeguard, documentation
//! tools and installation verification.

use std::process::Command;

use super::*;
use crate::fixtures::get_expected_scores;

pub fn test_server_startup() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start successfully");

    let response = client.initialize().expect("Initialize should succeed");
    assert!(
        response.get("result").is_some(),
        "Initialize response should have 'result'"
    );
}

pub fn test_code_health_scores() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let expected = get_expected_scores();

    for (file_path, (min_score, max_score)) in &expected {
        let full_path = repo_dir.join(file_path);
        let response = client
            .call_tool(
                "code_health_score",
                json!({"file_path": full_path.to_string_lossy()}),
                Duration::from_secs(60),
            )
            .unwrap_or_else(|e| panic!("Tool call failed for {file_path}: {e}"));

        let result_text = extract_result_text(&response);
        let score = extract_code_health_score(&result_text)
            .unwrap_or_else(|| panic!("No score found for {file_path}: {result_text}"));

        assert!(
            score >= *min_score && score <= *max_score,
            "{file_path}: score {score} not in range [{min_score}, {max_score}]"
        );
    }
}

pub fn test_code_health_review() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/services/order_processor.py");
    let response = client
        .call_tool(
            "code_health_review",
            json!({"file_path": test_file.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);

    assert!(
        result_text.len() > 50,
        "Review should return substantial content, got {} chars",
        result_text.len()
    );

    let lower = result_text.to_lowercase();
    assert!(
        lower.contains("code health") || lower.contains("complexity") || lower.contains("function"),
        "Review should contain Code Health terms"
    );
}

pub fn test_pre_commit_safeguard() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/utils/calculator.py");
    let original = std::fs::read_to_string(&test_file).expect("Read file");
    std::fs::write(&test_file, format!("{original}\n# Test modification\n")).expect("Write file");

    Command::new("git")
        .args(["add", &test_file.to_string_lossy()])
        .current_dir(&repo_dir)
        .output()
        .expect("git add");

    let response = client
        .call_tool(
            "pre_commit_code_health_safeguard",
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);

    assert!(result_text.len() > 20, "Safeguard should return content");

    let lower = result_text.to_lowercase();
    assert!(
        lower.contains("quality") || lower.contains("gate") || lower.contains("code health"),
        "Should contain quality gate info"
    );

    // Reset
    std::fs::write(&test_file, &original).expect("Restore file");
    Command::new("git")
        .args(["reset", "HEAD", &test_file.to_string_lossy()])
        .current_dir(&repo_dir)
        .output()
        .expect("git reset");
}

pub fn test_outside_git_repo() {
    let (command, env, _, _tmp) = setup();
    let standalone_dir = create_temp_dir("cs_mcp_standalone_").expect("temp dir");
    let standalone_file = standalone_dir.path().join("standalone.py");
    std::fs::write(&standalone_file, "def test():\n    pass\n").expect("write file");

    let mut client = make_client(&command, &env, standalone_dir.path());

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": standalone_file.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);

    assert!(!result_text.is_empty(), "Should get a response");
    assert!(
        !result_text.contains("NoneType") && !result_text.contains("Traceback"),
        "Should not contain crash errors"
    );
}

fn assert_documentation_tool(tool_name: &str, expected_terms: &[&str]) {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(tool_name, json!({}), Duration::from_secs(30))
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);

    assert!(result_text.len() > 100, "Should return documentation");

    let lower = result_text.to_lowercase();
    let terms_found = expected_terms.iter().filter(|t| lower.contains(*t)).count();
    assert!(
        terms_found >= 2,
        "Should contain expected terms for {tool_name}"
    );
}

pub fn test_explain_code_health() {
    assert_documentation_tool(
        "explain_code_health",
        &["code health", "maintainability", "quality"],
    );
}

pub fn test_explain_code_health_productivity() {
    assert_documentation_tool(
        "explain_code_health_productivity",
        &["productivity", "defect", "business", "code health"],
    );
}

pub fn test_verify_installation() {
    let (command, env, repo_dir, _tmp) = setup();
    std::fs::write(
        repo_dir.join("AGENTS.md"),
        include_str!("../../../docs/AGENTS-full.md"),
    )
    .expect("write agent instructions");
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "verify_installation",
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let lower = result_text.to_lowercase();

    assert!(
        lower.contains("installation verification"),
        "Should contain verification header"
    );
    assert!(
        lower.contains("[pass] runtime environment"),
        "Environment check should pass"
    );
    assert!(
        lower.contains("[pass] agent instructions"),
        "CodeHealth MCP agent instructions should be detected: {result_text}"
    );
    assert!(
        result_text.contains("checks passed"),
        "Should contain summary"
    );
}

pub fn test_verify_reports_git_repository() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "verify_installation",
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let lower = result_text.to_lowercase();

    assert!(
        lower.contains("[pass] git repository"),
        "Git repository check should pass"
    );
    assert!(lower.contains("git root"), "Should report git root path");
}

pub fn test_verify_non_repo_fails_git_check() {
    let (command, env, _, _tmp) = setup();
    let non_repo = create_temp_dir("cs_mcp_non_repo_").expect("temp dir");
    let non_repo_dir = non_repo.path().join("not_a_repo");
    std::fs::create_dir_all(&non_repo_dir).expect("create dir");

    let mut client = make_client(&command, &env, &non_repo_dir);

    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "verify_installation",
            json!({"git_repository_path": non_repo_dir.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let lower = result_text.to_lowercase();

    assert!(
        lower.contains("[fail] git repository"),
        "Git repository check should fail"
    );
    assert!(
        lower.contains("not inside a git repository"),
        "Should report not inside a git repository"
    );
}
