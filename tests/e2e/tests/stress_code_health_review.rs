//! Stress test for `code_health_review` under repeated invocations.
//!
//! Validates that a single MCP server instance handles many sequential
//! `code_health_review` calls without telemetry race conditions or failures.
//! Telemetry tracking is deliberately enabled to exercise the file-locking path.

use super::*;

const DEFAULT_ITERATIONS: usize = 50;
const TIMEOUT_SECS: u64 = 90;
const TELEMETRY_RACE_MARKERS: &[&str] = &["NoSuchFileException", "codescene-cli.log.jsonl"];

fn contains_telemetry_race_error(text: &str) -> bool {
    TELEMETRY_RACE_MARKERS.iter().all(|marker| text.contains(marker))
}

fn check_single_call(client: &mut MCPClient, file_path: &Path) -> (bool, bool) {
    let response = client.call_tool(
        "code_health_review",
        json!({"file_path": file_path.to_string_lossy()}),
        Duration::from_secs(TIMEOUT_SECS),
    );

    match &response {
        Ok(resp) => {
            let text = extract_result_text(resp);
            let has_race = contains_telemetry_race_error(&text);
            let failed = text.is_empty() || has_race;
            (failed, has_race)
        }
        Err(_) => (true, false),
    }
}

fn run_iterations(client: &mut MCPClient, file_path: &Path) -> (usize, usize) {
    let mut total_failures = 0;
    let mut telemetry_failures = 0;

    for i in 1..=DEFAULT_ITERATIONS {
        let (failed, has_race) = check_single_call(client, file_path);
        if has_race {
            telemetry_failures += 1;
        }
        if failed {
            total_failures += 1;
        }
        if i % 10 == 0 || i == DEFAULT_ITERATIONS {
            eprintln!("[{i}/{DEFAULT_ITERATIONS}] failures so far: {total_failures}");
        }
    }

    let stderr = client.get_stderr();
    if contains_telemetry_race_error(&stderr) {
        eprintln!("Telemetry race markers found in server stderr");
        telemetry_failures += 1;
        total_failures += 1;
    }

    (total_failures, telemetry_failures)
}

pub fn test_stress_code_health_review() {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);
    let temp_dir = create_temp_dir("cs_mcp_stress_").expect("temp dir");
    let repo_dir = create_git_repo(temp_dir.path(), &get_sample_files()).expect("git repo");

    let mut base = base_env();
    base.retain(|k, _| k != "CS_DISABLE_TRACKING");

    let env = backend.get_env(&base, &repo_dir);
    let env_vec: Vec<(String, String)> = env.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    let mut client = make_client(&command, &env_vec, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let review_target = repo_dir.join("src/services/order_processor.py");
    let (total_failures, telemetry_failures) =
        run_iterations(&mut client, &review_target);

    assert_eq!(
        total_failures, 0,
        "Stress test had {total_failures} failures ({telemetry_failures} telemetry)"
    );
}
