//! Platform-specific integration tests.
//!
//! Tests path handling for absolute paths, relative paths, symlinks,
//! spaces in paths, and Unicode characters in paths.

use super::*;

const TIMEOUT: Duration = Duration::from_secs(60);

fn run_score_test(repo_dir: &Path, file_path: &str) -> String {
    let (command, env, _, _tmp) = setup();
    let mut client = make_client(&command, &env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": file_path}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    extract_result_text(&response)
}

pub fn test_absolute_paths() {
    if is_docker() { return skip_if_docker("platform paths require host filesystem"); }
    let (_, _, repo_dir, _tmp) = setup();
    let abs_path = repo_dir
        .join("src/utils/calculator.py")
        .canonicalize()
        .unwrap_or_else(|_| repo_dir.join("src/utils/calculator.py"));

    let result = run_score_test(&repo_dir, &abs_path.to_string_lossy());

    assert!(!result.is_empty(), "Should return content");
    let lower = result.to_lowercase();
    assert!(
        lower.contains("score") || lower.contains("code health"),
        "Should contain score info: {result}"
    );
}

pub fn test_relative_paths() {
    if is_docker() { return skip_if_docker("platform paths require host filesystem"); }
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": "src/utils/calculator.py"}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(!result.is_empty(), "Should return content");
    let lower = result.to_lowercase();
    assert!(
        lower.contains("score") || lower.contains("code health"),
        "Should contain score info"
    );
}

pub fn test_symlinks() {
    if is_docker() { return skip_if_docker("platform paths require host filesystem"); }
    if cfg!(windows) {
        eprintln!("  SKIP: Symlink test skipped on Windows");
        return;
    }

    let (command, env, _, _tmp) = setup();
    let temp = create_temp_dir("cs_mcp_symlink_").expect("temp dir");
    let original = temp.path().join("original.py");
    std::fs::write(&original, "def test():\n    return 42\n").expect("write original");

    let symlink = temp.path().join("symlink.py");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&original, &symlink).expect("create symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&original, &symlink).expect("create symlink");

    let mut client = make_client(&command, &env, temp.path());
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": symlink.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(!result.is_empty(), "Should return content");
    assert!(
        !result.contains("Traceback"),
        "Should not contain crash errors"
    );
}

pub fn test_spaces_in_paths() {
    if is_docker() { return skip_if_docker("platform paths require host filesystem"); }
    let (command, env, _, _tmp) = setup();
    let temp = create_temp_dir("cs_mcp_spaces_").expect("temp dir");
    let dir = temp.path().join("directory with spaces");
    std::fs::create_dir_all(&dir).expect("create dir");
    let file = dir.join("file with spaces.py");
    std::fs::write(&file, "def function_with_spaces():\n    return 'test'\n").expect("write");

    let mut client = make_client(&command, &env, temp.path());
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(!result.is_empty(), "Should return content for path with spaces");
    assert!(
        !result.contains("Traceback"),
        "Should not contain crash errors"
    );
}

pub fn test_unicode_in_paths() {
    if is_docker() { return skip_if_docker("platform paths require host filesystem"); }
    let (command, env, _, _tmp) = setup();
    let temp = create_temp_dir("cs_mcp_unicode_").expect("temp dir");
    let dir = temp.path().join("t\u{00eb}st_\u{30c7}\u{30a3}\u{30ec}\u{30af}\u{30c8}\u{30ea}");
    std::fs::create_dir_all(&dir).expect("create dir");
    let file = dir.join("f\u{00ee}l\u{00e9}_\u{30d5}\u{30a1}\u{30a4}\u{30eb}.py");
    std::fs::write(&file, "def unicode_function():\n    return '\u{0442}\u{0435}\u{0441}\u{0442}'\n")
        .expect("write");

    let mut client = make_client(&command, &env, temp.path());
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(!result.is_empty(), "Should return content for Unicode path");
    assert!(
        !result.contains("Traceback"),
        "Should not contain crash errors"
    );
}
