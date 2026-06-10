//! E2E integration tests for the CodeScene MCP Server.
//!
//! These tests build the cs-mcp binary, launch it as a subprocess,
//! and communicate via JSON-RPC over stdio -- mirroring real-world usage.
//!
//! Run with: `cargo test --test e2e`
//!
//! Requires:
//! - `CS_ACCESS_TOKEN` environment variable set
//! - `git` available in PATH

mod mcp_client;
mod server_backends;
mod fixtures;
mod file_utils;
mod response_parsers;
mod tests;

use mcp_client::MCPClient;
use server_backends::{base_env, create_backend};
use fixtures::{get_sample_files, get_expected_scores};
use file_utils::{create_git_repo, create_temp_dir};
use response_parsers::{extract_result_text, extract_code_health_score};

use serde_json::json;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Shared setup: prepare backend and create a test repo.
pub fn setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_e2e_").expect("Failed to create temp dir");
    let sample_files = get_sample_files();
    let repo_dir =
        create_git_repo(temp_dir.path(), &sample_files).expect("Failed to create git repo");

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let env_vec: Vec<(String, String)> = env_map.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    (command, env_vec, repo_dir, temp_dir)
}

/// Find the release binary or build it.
pub fn find_or_build_executable() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("CS_MCP_EXECUTABLE") {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            ensure_executable(&p);
            return p;
        }
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary_name = if cfg!(windows) { "cs-mcp.exe" } else { "cs-mcp" };
    let release_binary = repo_root.join("target").join("release").join(binary_name);

    if release_binary.exists() {
        return release_binary;
    }

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(repo_root)
        .status()
        .expect("Failed to run cargo build");

    assert!(status.success(), "cargo build --release failed");
    assert!(release_binary.exists(), "Binary not found after build");

    release_binary
}

#[cfg(unix)]
fn ensure_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode();
        if mode & 0o111 == 0 {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o755));
        }
    }
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) {}

pub fn make_client(command: &[String], env: &[(String, String)], cwd: &Path) -> MCPClient {
    MCPClient::new(
        command.to_vec(),
        env.to_vec(),
        Some(cwd.to_string_lossy().to_string()),
    )
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_server_startup() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);

    assert!(client.start(), "Server should start successfully");

    let response = client.initialize().expect("Initialize should succeed");
    assert!(
        response.get("result").is_some(),
        "Initialize response should have 'result'"
    );
}

#[test]
fn test_code_health_scores() {
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

#[test]
fn test_code_health_review() {
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

#[test]
fn test_pre_commit_safeguard() {
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

#[test]
fn test_outside_git_repo() {
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
    assert!(terms_found >= 2, "Should contain expected terms for {tool_name}");
}

#[test]
fn test_explain_code_health() {
    assert_documentation_tool("explain_code_health", &["code health", "maintainability", "quality"]);
}

#[test]
fn test_explain_code_health_productivity() {
    assert_documentation_tool(
        "explain_code_health_productivity",
        &["productivity", "defect", "business", "code health"],
    );
}

#[test]
fn test_verify_installation() {
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
        lower.contains("installation verification"),
        "Should contain verification header"
    );
    assert!(
        lower.contains("[pass] runtime environment"),
        "Environment check should pass"
    );
    assert!(
        result_text.contains("checks passed"),
        "Should contain summary"
    );
}

#[test]
fn test_verify_reports_git_repository() {
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
    assert!(
        lower.contains("git root"),
        "Should report git root path"
    );
}

#[test]
fn test_verify_non_repo_fails_git_check() {
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

// ============================================================================
// Ported test modules
// ============================================================================

// --- Business Case ---
#[test]
fn test_business_case_basic_response() {
    tests::business_case::test_business_case_basic_response();
}

#[test]
fn test_business_case_contains_metrics() {
    tests::business_case::test_business_case_contains_metrics();
}

#[test]
fn test_business_case_no_file_errors() {
    tests::business_case::test_business_case_no_file_errors();
}

// --- Relative Paths ---
#[test]
fn test_relative_path_simple() {
    tests::relative_paths::test_relative_path_simple();
}

#[test]
fn test_relative_path_nested() {
    tests::relative_paths::test_relative_path_nested();
}

#[test]
fn test_relative_path_dot_prefix() {
    tests::relative_paths::test_relative_path_dot_prefix();
}

#[test]
fn test_relative_path_from_subdir() {
    tests::relative_paths::test_relative_path_from_subdir();
}

#[test]
fn test_mixed_slashes() {
    tests::relative_paths::test_mixed_slashes();
}

#[test]
fn test_absolute_path() {
    tests::relative_paths::test_absolute_path();
}

// --- Require Access Token ---
#[test]
fn test_guarded_tool_blocked_without_token() {
    tests::require_access_token::test_guarded_tool_blocked_without_token();
}

#[test]
fn test_explain_tool_blocked_without_token() {
    tests::require_access_token::test_explain_tool_blocked_without_token();
}

#[test]
fn test_get_config_works_without_token() {
    tests::require_access_token::test_get_config_works_without_token();
}

#[test]
fn test_set_config_works_without_token() {
    tests::require_access_token::test_set_config_works_without_token();
}

#[test]
fn test_guarded_tool_works_with_token() {
    tests::require_access_token::test_guarded_tool_works_with_token();
}

// --- Enabled Tools ---
#[test]
fn test_all_tools_without_filter() {
    tests::enabled_tools::test_all_tools_without_filter();
}

#[test]
fn test_filter_restricts_tools() {
    tests::enabled_tools::test_filter_restricts_tools();
}

#[test]
fn test_config_tools_always_present() {
    tests::enabled_tools::test_config_tools_always_present();
}

#[test]
fn test_set_enabled_tools_restart_warning() {
    tests::enabled_tools::test_set_enabled_tools_restart_warning();
}

#[test]
fn test_set_invalid_tool_name_warning() {
    tests::enabled_tools::test_set_invalid_tool_name_warning();
}

#[test]
fn test_get_enabled_tools_shows_available() {
    tests::enabled_tools::test_get_enabled_tools_shows_available();
}

// --- Configure ---
#[test]
fn test_config_tools_visible() {
    tests::configure::test_tools_visible();
}

#[test]
fn test_config_set_then_get() {
    tests::configure::test_set_then_get();
}

#[test]
fn test_config_sensitive_masking() {
    tests::configure::test_sensitive_masking();
}

#[test]
fn test_config_list_all() {
    tests::configure::test_list_all();
}

#[test]
fn test_config_invalid_key() {
    tests::configure::test_invalid_key();
}

#[test]
fn test_config_delete_value() {
    tests::configure::test_delete_value();
}

#[test]
fn test_config_env_override() {
    tests::configure::test_env_override();
}

#[test]
fn test_config_hidden_option_accessible_by_key() {
    tests::configure::test_hidden_option_accessible_by_key();
}

#[test]
fn test_config_standalone_hides_api_only() {
    tests::configure::test_standalone_hides_api_only();
}

#[test]
fn test_config_rejects_http_url() {
    tests::configure::test_set_config_rejects_http_url();
}

#[test]
fn test_config_accepts_https_url() {
    tests::configure::test_set_config_accepts_https_url();
}

#[test]
fn test_config_http_rejection_does_not_persist() {
    tests::configure::test_set_config_http_rejection_does_not_persist();
}

#[test]
fn test_config_non_url_key_unaffected() {
    tests::configure::test_set_config_non_url_key_unaffected();
}

// --- Standalone License ---
#[test]
fn test_standalone_hides_api_tools() {
    tests::standalone_license::test_standalone_hides_api_tools();
}

#[test]
fn test_standalone_keeps_cli_tools() {
    tests::standalone_license::test_standalone_keeps_cli_tools();
}

#[test]
fn test_pat_exposes_all_tools() {
    tests::standalone_license::test_pat_exposes_all_tools();
}

// --- Analyze Change Set ---
#[test]
fn test_change_set_passes_on_clean_branch() {
    tests::analyze_change_set::test_passes_on_clean_branch();
}

#[test]
fn test_change_set_fails_on_degraded_branch() {
    tests::analyze_change_set::test_fails_on_degraded_branch();
}

#[test]
fn test_change_set_fails_on_new_file_degraded() {
    tests::analyze_change_set::test_fails_on_new_file_with_degraded_health();
}

#[test]
fn test_change_set_passes_on_new_file_clean() {
    tests::analyze_change_set::test_passes_on_new_file_with_clean_health();
}

// --- Bundled Docs ---
#[test]
fn test_bundled_explain_code_health() {
    tests::bundled_docs::test_explain_code_health();
}

#[test]
fn test_bundled_explain_code_health_productivity() {
    tests::bundled_docs::test_explain_code_health_productivity();
}

#[test]
fn test_bundled_no_doc_file_errors() {
    tests::bundled_docs::test_no_doc_file_errors();
}

// --- Shutdown During Handshake ---
#[test]
fn test_stdin_closed_before_any_input() {
    tests::shutdown_during_handshake::test_stdin_closed_before_any_input();
}

#[test]
fn test_stdin_closed_after_initialize_request() {
    tests::shutdown_during_handshake::test_stdin_closed_after_initialize_request();
}

#[test]
fn test_stdin_closed_after_full_handshake() {
    tests::shutdown_during_handshake::test_stdin_closed_after_full_handshake();
}

#[test]
fn test_sigterm_before_any_input() {
    tests::shutdown_during_handshake::test_sigterm_before_any_input();
}

#[test]
fn test_sigterm_after_full_handshake() {
    tests::shutdown_during_handshake::test_sigterm_after_full_handshake();
}

// --- Docker Path Translation ---
#[test]
fn test_docker_verify_finds_git_repo() {
    tests::docker_path_translation::test_docker_verify_finds_git_repo();
}

#[test]
fn test_docker_code_health_score() {
    tests::docker_path_translation::test_docker_code_health_score();
}

#[test]
fn test_docker_pre_commit_safeguard() {
    tests::docker_path_translation::test_docker_pre_commit_safeguard();
}

#[test]
fn test_docker_code_health_review() {
    tests::docker_path_translation::test_docker_code_health_review();
}

// --- Platform Specific ---
#[test]
fn test_platform_absolute_paths() {
    tests::platform_specific::test_absolute_paths();
}

#[test]
fn test_platform_relative_paths() {
    tests::platform_specific::test_relative_paths();
}

#[test]
fn test_platform_symlinks() {
    tests::platform_specific::test_symlinks();
}

#[test]
fn test_platform_spaces_in_paths() {
    tests::platform_specific::test_spaces_in_paths();
}

#[test]
fn test_platform_unicode_in_paths() {
    tests::platform_specific::test_unicode_in_paths();
}

// --- Git Worktree ---
#[test]
fn test_worktree_code_health_score() {
    tests::git_worktree::test_worktree_code_health_score();
}

#[test]
fn test_worktree_code_health_review() {
    tests::git_worktree::test_worktree_code_health_review();
}

#[test]
fn test_worktree_pre_commit() {
    tests::git_worktree::test_worktree_pre_commit();
}

#[test]
fn test_worktree_absolute_paths() {
    tests::git_worktree::test_worktree_absolute_paths();
}

// --- Git Subtree ---
#[test]
fn test_subtree_code_health_score() {
    tests::git_subtree::test_subtree_code_health_score();
}

#[test]
fn test_subtree_code_health_review() {
    tests::git_subtree::test_subtree_code_health_review();
}

#[test]
fn test_subtree_pre_commit() {
    tests::git_subtree::test_subtree_pre_commit();
}

#[test]
fn test_subtree_absolute_paths() {
    tests::git_subtree::test_subtree_absolute_paths();
}

#[test]
fn test_subtree_main_repo_still_works() {
    tests::git_subtree::test_main_repo_still_works();
}

// --- Version Check ---
#[test]
fn test_version_tool_responds_when_github_unreachable() {
    tests::version_check::test_tool_responds_when_github_unreachable();
}

#[test]
fn test_version_no_version_update_noise() {
    tests::version_check::test_no_version_update_noise();
}

#[test]
fn test_version_response_time_acceptable() {
    tests::version_check::test_response_time_acceptable();
}

#[test]
fn test_version_info_appears_after_background_fetch() {
    tests::version_check::test_version_info_appears_after_background_fetch();
}

#[test]
fn test_version_disabled_no_banner() {
    tests::version_check::test_disabled_version_check_no_banner();
}

#[test]
fn test_version_disabled_no_network_traffic() {
    tests::version_check::test_disabled_version_check_no_network_traffic();
}

// --- Analytics Tracking ---
#[test]
fn test_analytics_tool_responds_when_unreachable() {
    tests::analytics_tracking::test_tool_responds_when_analytics_unreachable();
}

#[test]
fn test_analytics_response_time_not_delayed() {
    tests::analytics_tracking::test_response_time_not_delayed_by_analytics();
}

#[test]
fn test_analytics_events_are_sent() {
    tests::analytics_tracking::test_analytics_events_are_sent();
}

#[test]
fn test_analytics_disabled_tracking_sends_no_events() {
    tests::analytics_tracking::test_disabled_tracking_sends_no_events();
}

#[test]
fn test_analytics_disabled_tracking_returns_valid_results() {
    tests::analytics_tracking::test_disabled_tracking_returns_valid_results();
}

#[test]
fn test_analytics_enriched_common_properties() {
    tests::analytics_tracking::test_enriched_event_contains_common_properties();
}

#[test]
fn test_analytics_enriched_tool_specific_properties() {
    tests::analytics_tracking::test_enriched_event_contains_tool_specific_properties();
}

// --- Analytics Tracking (enriched tool-specific) ---
#[test]
fn test_analytics_enriched_review_event() {
    tests::analytics_tracking::test_enriched_review_event();
}

#[test]
fn test_analytics_enriched_pre_commit_event() {
    tests::analytics_tracking::test_enriched_pre_commit_event();
}

#[test]
fn test_analytics_enriched_analyze_change_set_event() {
    tests::analytics_tracking::test_enriched_analyze_change_set_event();
}

#[test]
fn test_analytics_enriched_pre_commit_with_findings() {
    tests::analytics_tracking::test_enriched_pre_commit_event_with_findings();
}

#[test]
fn test_analytics_enriched_change_set_with_findings() {
    tests::analytics_tracking::test_enriched_analyze_change_set_event_with_findings();
}

// --- Analytics Environment Override ---
#[test]
fn test_analytics_default_environment_is_sent() {
    tests::analytics_environment_override::test_default_environment_is_sent();
}

#[test]
fn test_analytics_overridden_environment_is_sent() {
    tests::analytics_environment_override::test_overridden_environment_is_sent();
}

// --- CloudFront Headers ---
#[test]
fn test_cloudfront_api_client_headers() {
    tests::cloudfront_headers::test_api_client_headers();
}

// --- Error Logging ---
#[test]
fn test_error_telemetry_sends_only_kind() {
    tests::error_logging::test_error_telemetry_sends_only_kind();
}

#[test]
fn test_error_telemetry_invalid_token() {
    tests::error_logging::test_error_telemetry_invalid_token();
}

#[test]
fn test_error_logged_to_file() {
    tests::error_logging::test_error_logged_to_file();
}

#[test]
fn test_file_logging_disabled_when_zero() {
    tests::error_logging::test_file_logging_disabled_when_zero();
}

#[test]
fn test_unsupported_file_type_detail_in_telemetry() {
    tests::error_logging::test_unsupported_file_type_detail_in_telemetry();
}

// --- Skill Resources ---
#[test]
fn test_skill_init_capabilities() {
    tests::skill_resources::test_init_capabilities();
}

#[test]
fn test_skill_list_resources() {
    tests::skill_resources::test_list_resources();
}

#[test]
fn test_skill_read_skill_md() {
    tests::skill_resources::test_read_skill_md();
}

#[test]
fn test_skill_read_manifest() {
    tests::skill_resources::test_read_manifest();
}

#[test]
fn test_skill_list_resource_templates() {
    tests::skill_resources::test_list_resource_templates();
}

#[test]
fn test_skill_read_error_cases() {
    tests::skill_resources::test_read_error_cases();
}

#[test]
fn test_skill_list_skills_tool() {
    tests::skill_resources::test_list_skills_tool();
}

#[test]
fn test_skill_get_manifest_tool() {
    tests::skill_resources::test_get_skill_manifest_tool();
}

#[test]
fn test_skill_download_tool() {
    tests::skill_resources::test_download_skill_tool();
}

#[test]
fn test_skill_sync_tool() {
    tests::skill_resources::test_sync_skills_tool();
}

// --- SSL CLI Certificate Passthrough ---
#[test]
fn test_ssl_cs_certs_passed_to_cli() {
    tests::ssl_cli_truststore::test_cs_certs_passed_to_cli();
}

#[test]
fn test_ssl_cs_certs_missing_without_ca_bundle() {
    tests::ssl_cli_truststore::test_cs_certs_missing_without_ca_bundle();
}

#[test]
fn test_ssl_cs_certs_not_set_when_ca_bundle_path_invalid() {
    tests::ssl_cli_truststore::test_cs_certs_not_set_when_ca_bundle_path_invalid();
}

// --- SSL API CA Bundle ---
#[test]
fn test_api_uses_ca_bundle() {
    tests::ssl_api_ca_bundle::test_api_uses_ca_bundle();
}

#[test]
fn test_api_fails_without_ca_bundle() {
    tests::ssl_api_ca_bundle::test_api_fails_without_ca_bundle();
}

#[test]
fn test_api_fails_with_invalid_ca_bundle_path() {
    tests::ssl_api_ca_bundle::test_api_fails_with_invalid_ca_bundle_path();
}

// --- SSL CA Bundle Path Formats ---
#[test]
fn test_ssl_path_baseline_fails_without_ca_bundle() {
    tests::ssl_ca_bundle_path_formats::test_baseline_fails_without_ca_bundle();
}

#[test]
fn test_ssl_path_canonical_succeeds() {
    tests::ssl_ca_bundle_path_formats::test_canonical_path_succeeds();
}

#[test]
fn test_ssl_path_forward_slash_succeeds() {
    tests::ssl_ca_bundle_path_formats::test_forward_slash_path_succeeds();
}

#[cfg(windows)]
#[test]
fn test_ssl_path_backslash_succeeds() {
    tests::ssl_ca_bundle_path_formats::test_backslash_path_succeeds();
}

#[test]
fn test_ssl_path_nonexistent_ca_bundle_fails() {
    tests::ssl_ca_bundle_path_formats::test_nonexistent_ca_bundle_path_fails();
}

#[test]
fn test_ssl_path_set_config_ca_bundle_applies_immediately() {
    tests::ssl_ca_bundle_path_formats::test_set_config_ca_bundle_applies_immediately();
}

// --- Stress Test ---
#[test]
#[ignore] // Long-running; run with --ignored
fn test_stress_review() {
    tests::stress_code_health_review::test_stress_code_health_review();
}
