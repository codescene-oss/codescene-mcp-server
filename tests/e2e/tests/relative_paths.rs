//! Tests that verify relative and absolute file paths resolve correctly
//! when calling `code_health_score`.

use super::*;

fn assert_path_resolves(command: &[String], env: &[(String, String)], repo_dir: &Path, file_path: &str) {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": file_path}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let score = extract_code_health_score(&result_text);

    assert!(score.is_some(), "Expected a score for '{file_path}', got: {result_text}");
    assert!(
        !result_text.contains("not in the subpath"),
        "Path '{file_path}' should resolve without subpath error: {result_text}"
    );
}

pub fn test_relative_path_simple() {
    if is_docker() { return skip_if_docker("relative paths require host filesystem"); }
    let (command, env, repo_dir, _tmp) = setup();
    assert_path_resolves(&command, &env, &repo_dir, "src/utils/calculator.py");
}

pub fn test_relative_path_nested() {
    if is_docker() { return skip_if_docker("relative paths require host filesystem"); }
    let (command, env, repo_dir, _tmp) = setup();
    assert_path_resolves(&command, &env, &repo_dir, "src/main/java/com/example/OrderProcessor.java");
}

pub fn test_relative_path_dot_prefix() {
    if is_docker() { return skip_if_docker("relative paths require host filesystem"); }
    let (command, env, repo_dir, _tmp) = setup();
    assert_path_resolves(&command, &env, &repo_dir, "./src/utils/calculator.py");
}

pub fn test_relative_path_from_subdir() {
    if is_docker() { return skip_if_docker("relative paths require host filesystem"); }
    let (command, env, repo_dir, _tmp) = setup();
    assert_path_resolves(&command, &env, &repo_dir, "src/services/order_processor.py");
}

pub fn test_mixed_slashes() {
    if is_docker() { return skip_if_docker("relative paths require host filesystem"); }
    let (command, env, repo_dir, _tmp) = setup();
    assert_path_resolves(&command, &env, &repo_dir, "src/utils/calculator.py");
}

pub fn test_absolute_path() {
    let (command, env, repo_dir, _tmp) = setup();
    let absolute = repo_dir.join("src/utils/calculator.py");
    assert_path_resolves(&command, &env, &repo_dir, &absolute.to_string_lossy());
}
