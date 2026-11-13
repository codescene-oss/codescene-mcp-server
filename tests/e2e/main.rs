//! E2E integration tests for the CodeHealth MCP Server.
//!
//! These tests build the cs-mcp binary, launch it as a subprocess,
//! and communicate via JSON-RPC over stdio -- mirroring real-world usage.
//!
//! Run with: `cargo test --test e2e`
//!
//! Requires:
//! - `CS_ACCESS_TOKEN` environment variable set
//! - `git` available in PATH
//!
//! Telemetry shutdown diagnostics use a dummy token and local endpoints:
//! `cargo test --test e2e test_analytics_shutdown -- --nocapture`.
//! They require queued delivery on SIGTERM and characterize SIGKILL loss.

mod file_utils;
mod fixtures;
mod mcp_client;
mod response_parsers;
mod server_backends;
mod tests;

use file_utils::{create_git_repo, create_temp_dir};
use fixtures::get_sample_files;
use mcp_client::MCPClient;
use server_backends::{base_env, create_backend};

use std::path::Path;
use std::process::Command;

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
    let binary_name = if cfg!(windows) {
        "cs-mcp.exe"
    } else {
        "cs-mcp"
    };
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

/// Registers `#[test]` wrappers that delegate to `pub fn` tests in `tests::<module>`.
///
/// Use `name => target` when the wrapper name differs from the module function,
/// and prefix an entry with attributes such as `#[ignore]` or `#[cfg(windows)]`.
macro_rules! e2e_tests {
    ($module:ident { $($(#[$meta:meta])* $name:ident $(=> $target:ident)?),* $(,)? }) => {
        $(
            $(#[$meta])*
            #[test]
            fn $name() {
                e2e_tests!(@call $module, $name $(, $target)?);
            }
        )*
    };
    (@call $module:ident, $name:ident) => {
        tests::$module::$name()
    };
    (@call $module:ident, $name:ident, $target:ident) => {
        tests::$module::$target()
    };
}

// ============================================================================
// Tests
// ============================================================================

// --- Core Tools ---
e2e_tests!(core_tools {
    test_server_startup,
    test_code_health_scores,
    test_code_health_review,
    test_pre_commit_safeguard,
    test_outside_git_repo,
    test_explain_code_health,
    test_explain_code_health_productivity,
    test_verify_installation,
    test_verify_reports_git_repository,
    test_verify_non_repo_fails_git_check,
});

e2e_tests!(pre_commit_code_health_safeguard {
    test_pre_commit_reports_no_issues_found_for_clean_staged_changes => test_reports_no_issues_found_for_clean_staged_changes,
    test_pre_commit_reports_no_files_modified_for_empty_staging_area => test_reports_no_files_modified_for_empty_staging_area,
});

// --- Business Case ---
e2e_tests!(business_case {
    test_business_case_basic_response,
    test_business_case_contains_metrics,
    test_business_case_no_file_errors,
    test_business_case_user_selected_target,
    test_business_case_rejects_out_of_range_target,
    test_business_case_target_not_above_current,
});

// --- Relative Paths ---
e2e_tests!(relative_paths {
    test_relative_path_simple,
    test_relative_path_nested,
    test_relative_path_dot_prefix,
    test_relative_path_from_subdir,
    test_mixed_slashes,
    test_absolute_path,
});

// --- Literal Paths ---
e2e_tests!(literal_paths {
    test_literal_paths_code_health_score,
    test_literal_paths_code_health_review,
});

// --- Require Access Token ---
e2e_tests!(require_access_token {
    test_guarded_tool_blocked_without_token,
    test_explain_tool_blocked_without_token,
    test_get_config_works_without_token,
    test_set_config_works_without_token,
    test_guarded_tool_works_with_token,
});

// --- Enabled Tools ---
e2e_tests!(enabled_tools {
    test_all_tools_without_filter,
    test_filter_restricts_tools,
    test_config_tools_always_present,
    test_set_enabled_tools_restart_warning,
    test_set_invalid_tool_name_warning,
    test_get_enabled_tools_shows_available,
});

// --- Tool Annotations ---
e2e_tests!(tool_annotations {
    test_tool_read_only_annotations,
});

// --- Configure ---
e2e_tests!(configure {
    test_config_tools_visible => test_tools_visible,
    test_config_set_then_get => test_set_then_get,
    test_config_sensitive_masking => test_sensitive_masking,
    test_config_list_all => test_list_all,
    test_config_invalid_key => test_invalid_key,
    test_config_delete_value => test_delete_value,
    test_config_env_override => test_env_override,
    test_config_hidden_option_accessible_by_key => test_hidden_option_accessible_by_key,
    test_config_standalone_hides_api_only => test_standalone_hides_api_only,
    test_config_rejects_http_url => test_set_config_rejects_http_url,
    test_config_accepts_https_url => test_set_config_accepts_https_url,
    test_config_http_rejection_does_not_persist => test_set_config_http_rejection_does_not_persist,
    test_config_non_url_key_unaffected => test_set_config_non_url_key_unaffected,
});

// --- Rules Config ---
e2e_tests!(rules_config {
    test_rules_config_tools_listed,
    test_rules_config_validate_reports_valid_config => test_validate_reports_valid_config,
    test_rules_config_list_thresholds_returns_defaults => test_list_thresholds_returns_defaults,
    test_rules_config_list_thresholds_rejects_unknown_language => test_list_thresholds_rejects_unknown_language,
    test_rules_config_set_rule_disable_and_enable_persist => test_set_rule_disable_and_enable_persist,
    test_rules_config_set_threshold_persists_value => test_set_threshold_persists_value,
    test_rules_config_set_threshold_rejects_invalid_value => test_set_threshold_rejects_invalid_value,
    test_rules_config_set_then_validate_roundtrip => test_set_then_validate_roundtrip,
    test_rules_config_relative_config_path_is_rejected => test_relative_config_path_is_rejected,
    test_rules_config_works_without_access_token => test_works_without_access_token,
});

// --- Standalone License ---
e2e_tests!(standalone_license {
    test_standalone_hides_api_tools,
    test_standalone_keeps_cli_tools,
    test_pat_exposes_all_tools,
});

// --- Analyze Change Set ---
e2e_tests!(analyze_change_set {
    test_change_set_passes_on_clean_branch => test_passes_on_clean_branch,
    test_change_set_fails_on_degraded_branch => test_fails_on_degraded_branch,
    test_change_set_fails_on_new_file_degraded => test_fails_on_new_file_with_degraded_health,
    test_change_set_passes_on_new_file_clean => test_passes_on_new_file_with_clean_health,
    test_change_set_reports_no_issues_found_for_clean_change_set => test_reports_no_issues_found_for_clean_change_set,
    test_change_set_reports_no_files_modified_for_empty_change_set => test_reports_no_files_modified_for_empty_change_set,
});

// --- Bundled Docs ---
e2e_tests!(bundled_docs {
    test_bundled_explain_code_health => test_explain_code_health,
    test_bundled_explain_code_health_productivity => test_explain_code_health_productivity,
    test_bundled_no_doc_file_errors => test_no_doc_file_errors,
});

e2e_tests!(mcp_usage_overview {
    test_mcp_usage_overview_tool_and_resource => test_usage_tool_and_resource,
});

// --- Discovery Fallback ---
e2e_tests!(discovery_fallback {
    test_server_discover_falls_back_without_exiting,
});

// --- Shutdown During Handshake ---
e2e_tests!(shutdown_during_handshake {
    test_stdin_closed_before_any_input,
    test_stdin_closed_after_initialize_request,
    test_stdin_closed_after_full_handshake,
    test_sigterm_before_any_input,
    test_sigterm_after_full_handshake,
});

// --- Docker Path Translation ---
e2e_tests!(docker_path_translation {
    test_docker_verify_finds_git_repo,
    test_docker_code_health_score,
    test_docker_pre_commit_safeguard,
    test_docker_code_health_review,
});

// --- Platform Specific ---
e2e_tests!(platform_specific {
    test_platform_absolute_paths => test_absolute_paths,
    test_platform_relative_paths => test_relative_paths,
    test_platform_symlinks => test_symlinks,
    test_platform_spaces_in_paths => test_spaces_in_paths,
    test_platform_unicode_in_paths => test_unicode_in_paths,
});

// --- Git Worktree ---
e2e_tests!(git_worktree {
    test_worktree_code_health_score,
    test_worktree_code_health_review,
    test_worktree_pre_commit,
    test_worktree_absolute_paths,
});

// --- Git Subtree ---
e2e_tests!(git_subtree {
    test_subtree_code_health_score,
    test_subtree_code_health_review,
    test_subtree_pre_commit,
    test_subtree_absolute_paths,
    test_subtree_main_repo_still_works => test_main_repo_still_works,
});

// --- Version Check ---
e2e_tests!(version_check {
    test_version_tool_responds_when_github_unreachable => test_tool_responds_when_github_unreachable,
    test_version_no_version_update_noise => test_no_version_update_noise,
    test_version_response_time_acceptable => test_response_time_acceptable,
    test_version_info_appears_after_background_fetch,
    test_version_disabled_no_banner => test_disabled_version_check_no_banner,
    test_version_disabled_no_network_traffic => test_disabled_version_check_no_network_traffic,
});

// --- Analytics Tracking ---
e2e_tests!(analytics_tracking {
    test_analytics_tool_responds_when_unreachable => test_tool_responds_when_analytics_unreachable,
    test_analytics_response_time_not_delayed => test_response_time_not_delayed_by_analytics,
    test_analytics_events_are_sent,
    test_analytics_event_contains_project_ids,
});

// --- Repository Project Matching ---
e2e_tests!(repository_project_matching {
    test_mock_project_matching_scp_ssh,
    test_mock_project_matching_ssh_url,
    test_mock_project_matching_https,
    test_mock_project_matching_onprem_optional_account_ids,
    test_mock_project_matching_multiple_fetch_urls,
    test_mock_project_matching_fetch_and_push_urls,
    test_mock_project_matching_unions_and_deduplicates_remotes,
    test_mock_project_matching_instead_of,
    test_mock_project_matching_push_instead_of,
    test_mock_project_matching_quoted_git_config,
    test_mock_project_matching_ssh_host_alias,
    test_mock_project_matching_invalid_ssh_config,
    test_mock_project_matching_unmatched_ssh_alias,
    test_mock_project_matching_no_remotes,
    test_mock_project_matching_unsupported_remote,
    test_mock_project_matching_worktree,
    test_mock_project_matching_worktree_subdirectory,
    test_matches_repository_from_active_account,
    test_matches_repository_from_other_account,
    test_unions_projects_across_accounts,
    test_matches_worktree_projects_from_active_account,
    test_matches_worktree_subdirectory_projects_from_other_account,
});

e2e_tests!(analytics_tracking {
    test_analytics_disabled_tracking_sends_no_events => test_disabled_tracking_sends_no_events,
    test_analytics_disabled_tracking_returns_valid_results => test_disabled_tracking_returns_valid_results,
    test_analytics_enriched_common_properties => test_enriched_event_contains_common_properties,
    test_analytics_agent_instruction_properties => test_agent_instruction_properties_reflect_repository_guidance,
    test_analytics_enriched_tool_specific_properties => test_enriched_event_contains_tool_specific_properties,
});

// --- Analytics Tracking (enriched tool-specific) ---
e2e_tests!(analytics_tracking {
    test_analytics_enriched_review_event => test_enriched_review_event,
    test_analytics_repository_file_count_performance => test_repository_file_count_performance,
    test_analytics_enriched_pre_commit_event => test_enriched_pre_commit_event,
    test_analytics_enriched_analyze_change_set_event => test_enriched_analyze_change_set_event,
    test_analytics_enriched_pre_commit_with_findings => test_enriched_pre_commit_event_with_findings,
    test_analytics_enriched_change_set_with_findings => test_enriched_analyze_change_set_event_with_findings,
});

// --- Analytics Environment Override ---
e2e_tests!(analytics_environment_override {
    test_analytics_default_environment_is_sent => test_default_environment_is_sent,
    test_analytics_overridden_environment_is_sent => test_overridden_environment_is_sent,
});

// --- CloudFront Headers ---
e2e_tests!(cloudfront_headers {
    test_cloudfront_api_client_headers => test_api_client_headers,
});

// --- Error Logging ---
e2e_tests!(error_logging {
    test_error_telemetry_sends_only_kind,
    test_error_telemetry_invalid_token,
    test_error_logged_to_file,
    test_file_logging_disabled_when_zero,
    test_unsupported_file_type_detail_in_telemetry,
});

// --- Skill Resources ---
e2e_tests!(skill_resources {
    test_skill_init_capabilities => test_init_capabilities,
    test_skill_list_resources => test_list_resources,
    test_skill_read_skill_md => test_read_skill_md,
    test_skill_read_manifest => test_read_manifest,
    test_skill_list_resource_templates => test_list_resource_templates,
    test_skill_read_error_cases => test_read_error_cases,
    test_skill_list_skills_tool => test_list_skills_tool,
    test_skill_get_manifest_tool => test_get_skill_manifest_tool,
    test_skill_download_tool => test_download_skill_tool,
    test_skill_sync_tool => test_sync_skills_tool,
});

// --- SSL CLI Truststore ---
e2e_tests!(ssl_cli_truststore {
    test_ssl_truststore_args_injected => test_truststore_args_are_injected,
    test_ssl_truststore_args_missing_without_cert => test_truststore_args_missing_without_cert,
    test_ssl_truststore_args_not_injected_when_ca_bundle_path_invalid => test_truststore_args_not_injected_when_ca_bundle_path_invalid,
});

// --- SSL CLI CA Bundle ---
e2e_tests!(ssl_cli_ca_bundle {
    test_ssl_cli_connectivity_passes_with_ca_bundle => test_cli_connectivity_passes_with_ca_bundle,
    test_ssl_cli_connectivity_fails_without_ca_bundle => test_cli_connectivity_fails_without_ca_bundle,
});

// --- SSL API CA Bundle ---
e2e_tests!(ssl_api_ca_bundle {
    test_api_uses_ca_bundle,
    test_api_fails_without_ca_bundle,
    test_api_fails_with_invalid_ca_bundle_path,
});

// --- SSL CA Bundle Path Formats ---
e2e_tests!(ssl_ca_bundle_path_formats {
    test_ssl_path_baseline_fails_without_ca_bundle => test_baseline_fails_without_ca_bundle,
    test_ssl_path_canonical_succeeds => test_canonical_path_succeeds,
    test_ssl_path_forward_slash_succeeds => test_forward_slash_path_succeeds,
    #[cfg(windows)] test_ssl_path_backslash_succeeds => test_backslash_path_succeeds,
    test_ssl_path_nonexistent_ca_bundle_fails => test_nonexistent_ca_bundle_path_fails,
    test_ssl_path_set_config_ca_bundle_applies_immediately => test_set_config_ca_bundle_applies_immediately,
});

// --- OAuth Login ---
e2e_tests!(oauth_login {
    test_login_skips_when_pat_configured,
    test_login_reuses_existing_session,
    test_login_interactive_flow_persists_token,
    test_login_fetches_token_when_login_omits_access_token,
    test_failed_login_persists_signed_out_state,
    test_persisted_oauth_reused_by_second_process,
    test_expiry_without_token_triggers_refresh,
    test_pat_takes_precedence_over_oauth,
    test_logout_clears_oauth_after_login,
    test_logout_notes_pat_still_configured,
    test_switch_account_reuses_cli_slot,
    test_switch_account_lists_without_args,
    test_switch_account_switches_by_name,
});

// --- OAuth Login Flow (real CLI + mock IdP, host-only) ---
e2e_tests!(oauth_login_flow {
    test_oauth_authorization_code_flow_persists_token,
    test_oauth_flow_fails_when_routes_missing,
});

// --- Stress Test ---
e2e_tests!(stress_code_health_review {
    // Long-running; run with --ignored
    #[ignore] test_stress_review => test_stress_code_health_review,
});
