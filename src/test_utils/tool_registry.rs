use std::collections::HashMap;
use std::sync::Arc;


use super::*;
use crate::config::{self, ConfigData};
use crate::environment;
use crate::version_checker::VersionChecker;
use crate::{remove_docker_unsupported_tools, API_ONLY_TOOLS};

#[test]
fn api_only_tools_has_expected_entries() {
    assert!([
        "select_project",
        "code_ownership_for_path",
        "show_mcp_usage_overview"
    ]
    .iter()
    .all(|tool| API_ONLY_TOOLS.contains(tool)));
    assert_eq!(API_ONLY_TOOLS.len(), 7);
}

#[test]
fn new_api_mode_keeps_all_tools() {
    let server = make_server(false);
    assert!(!server.is_standalone);
}

#[test]
fn new_standalone_mode_sets_flag() {
    let server = make_server(true);
    assert!(server.is_standalone);
}

#[test]
fn new_stores_instance_id() {
    let server = make_server(false);
    assert_eq!(server.instance_id, "test-instance");
}

#[tokio::test]
async fn login_dispatch_method_delegates_to_login_handler() {
    use rmcp::handler::server::wrapper::Parameters;

    let _g = set_token("existing-token");
    let server = make_server_with_mocks(
        false,
        MockCliRunner::with_responses(vec![]),
        MockHttpClient::new(vec![]),
    );
    let result = server
        .login(Parameters(crate::tools::LoginParam {}))
        .await
        .unwrap();
    assert!(
        result_text(&result).contains("CS_ACCESS_TOKEN is already configured"),
        "got: {}",
        result_text(&result)
    );
}

#[tokio::test]
async fn logout_dispatch_method_delegates_to_logout_handler() {
    use rmcp::handler::server::wrapper::Parameters;

    let _g = clear_token();
    std::env::set_var("CS_OAUTH_TOKEN", "oau-dispatch");
    std::env::set_var("CS_OAUTH_EXPIRES_AT", "9999999999");
    let server = make_server_with_mocks(
        false,
        MockCliRunner::with_ok(r#"{"status":"signed_out","access-token":null,"api-url":null}"#),
        MockHttpClient::new(vec![]),
    );
    let result = server
        .logout(Parameters(crate::tools::LogoutParam {}))
        .await
        .unwrap();
    assert!(
        result_text(&result).contains("Successfully signed out"),
        "got: {}",
        result_text(&result)
    );
    std::env::remove_var("CS_OAUTH_TOKEN");
    std::env::remove_var("CS_OAUTH_EXPIRES_AT");
}

#[tokio::test]
async fn switch_account_dispatch_method_delegates_to_handler() {
    use rmcp::handler::server::wrapper::Parameters;

    let _g = clear_token();
    let expires_at = crate::auth::now_epoch_secs() + 3600;
    std::env::set_var("CS_OAUTH_TOKEN", "tok");
    std::env::set_var("CS_OAUTH_EXPIRES_AT", expires_at.to_string());
    std::env::set_var("CS_OAUTH_ACCOUNT_ID", "42");
    // Queue a signed-in response so a concurrent env race that forces a CLI
    // token lookup still completes instead of panicking on an empty mock.
    let signed_in = format!(
        r#"{{"status":"signed_in","access-token":"tok","api-url":"https://api.codescene.io/api","expires-at":{expires_at},"account-id":42}}"#
    );
    let server = make_server_with_mocks(
        false,
        MockCliRunner::with_ok(&signed_in),
        MockHttpClient::new(vec![]),
    );
    let result = server
        .switch_account(Parameters(crate::tools::SwitchAccountParam {
            account_id: Some(42),
            name: None,
        }))
        .await
        .unwrap();
    let text = result_text(&result);
    assert!(
        text.contains("Already signed in") || text.contains("Switched to CodeScene account 42"),
        "got: {text}"
    );
    std::env::remove_var("CS_OAUTH_TOKEN");
    std::env::remove_var("CS_OAUTH_EXPIRES_AT");
    std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
    std::env::remove_var("CS_ACCOUNT_ID");
}

#[tokio::test]
async fn verify_installation_dispatch_method_delegates_to_handler() {
    use rmcp::handler::server::wrapper::Parameters;

    let _g = set_token("existing-token");
    let server = make_server_with_mocks(
        false,
        MockCliRunner::with_responses(vec![Ok("{}".to_string()), Ok("{}".to_string())]),
        MockHttpClient::always(crate::http::HttpResponse::ok(r#"[{"id":1}]"#)),
    );
    let result = server
        .verify_installation(Parameters(crate::tools::GitRepoParam {
            git_repository_path: "/tmp/project".to_string(),
        }))
        .await
        .unwrap();
    assert!(
        result_text(&result).contains("CLI Connectivity"),
        "got: {}",
        result_text(&result)
    );
}

#[tokio::test]
async fn maybe_version_warning_returns_text_when_no_cache() {
    assert_eq!(
        make_server(false).maybe_version_warning("hello").await,
        "hello"
    );
}

#[tokio::test]
async fn maybe_version_warning_prepends_warning_when_outdated() {
    let server = make_server_with_version("1.0.0", "2.0.0", true).await;
    let result = server.maybe_version_warning("body text").await;
    assert!(result.contains("VERSION UPDATE AVAILABLE"));
    assert!(result.contains("body text"));
}

// --- Tool filtering via enabled_tools ---
fn tool_names(server: &CodeSceneServer) -> Vec<String> {
    server
        .tool_router
        .list_all()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

const READ_ONLY_TOOLS: &[&str] = &[
    "show_mcp_usage_overview",
    "explain_code_health",
    "explain_code_health_productivity",
    "code_health_review",
    "code_health_score",
    "pre_commit_code_health_safeguard",
    "analyze_change_set",
    "code_health_refactoring_business_case",
    "rules_config_validate",
    "rules_config_list_thresholds",
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
    "get_config",
    "list_skills",
    "get_skill_manifest",
    "verify_installation",
];

const MUTATING_TOOLS: &[&str] = &[
    "rules_config_set_rule",
    "rules_config_set_threshold",
    "set_config",
    "login",
    "logout",
    "switch_account",
    "download_skill",
    "sync_skills",
];

#[test]
fn every_tool_declares_its_read_only_behavior() {
    let router = CodeSceneServer::tool_router();
    let tools = router.list_all();

    assert_eq!(tools.len(), READ_ONLY_TOOLS.len() + MUTATING_TOOLS.len());
    for (names, expected) in [(READ_ONLY_TOOLS, true), (MUTATING_TOOLS, false)] {
        for name in names {
            let tool = tools
                .iter()
                .find(|tool| tool.name == *name)
                .unwrap_or_else(|| panic!("missing tool {name}"));
            let read_only = tool
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.read_only_hint);
            assert_eq!(read_only, Some(expected), "wrong readOnlyHint for {name}");
        }
    }
}

fn make_server_with_enabled_tools(is_standalone: bool, enabled_tools: &str) -> CodeSceneServer {
    let mut values = HashMap::new();
    values.insert("enabled_tools".to_string(), enabled_tools.to_string());
    CodeSceneServer::new(ServerDeps {
        config_data: ConfigData {
            instance_id: Some("test-filter".to_string()),
            values,
        },
        instance_id: "test-filter".to_string(),
        is_standalone,
        version_checker: VersionChecker::new("dev"),
        auth_manager: AuthManager::new(),
        cli_runner: Arc::new(cli::ProductionCliRunner),
        http_client: Arc::new(http::ReqwestClient),
        validator: Arc::new(MockValidator::passing()),
    })
}

fn assert_has_config_tools(names: &[String]) {
    assert!(
        names.contains(&"get_config".to_string()),
        "missing get_config"
    );
    assert!(
        names.contains(&"set_config".to_string()),
        "missing set_config"
    );
    assert!(names.contains(&"login".to_string()), "missing login");
    assert!(names.contains(&"logout".to_string()), "missing logout");
    assert!(
        names.contains(&"switch_account".to_string()),
        "missing switch_account"
    );
}

fn assert_tool_count_and_config(names: &[String], expected: usize) {
    assert_eq!(
        names.len(),
        expected,
        "expected {expected} tools, got: {names:?}"
    );
    assert_has_config_tools(names);
}

#[test]
fn enabled_tools_unset_keeps_all_tools() {
    let _lock = config::lock_test_env();
    std::env::remove_var("CS_ENABLED_TOOLS");
    let server = make_server(false);
    let names = tool_names(&server);
    assert_tool_count_and_config(&names, 28);
    assert!(names.contains(&"code_health_review".to_string()));
    assert!(names.contains(&"show_mcp_usage_overview".to_string()));
}

#[test]
fn enabled_tools_filters_to_allowlist() {
    let _lock = config::lock_test_env();
    std::env::remove_var("CS_ENABLED_TOOLS");
    let server = make_server_with_enabled_tools(false, "code_health_review,code_health_score");
    let names = tool_names(&server);
    // Should have the 2 enabled tools + 5 always-on = 7
    assert_tool_count_and_config(&names, 7);
    assert!(names.contains(&"code_health_review".to_string()));
    assert!(names.contains(&"code_health_score".to_string()));
}

#[test]
fn enabled_tools_cannot_remove_config_tools() {
    let _lock = config::lock_test_env();
    std::env::remove_var("CS_ENABLED_TOOLS");
    // Only enable one tool — config tools must still be present
    let server = make_server_with_enabled_tools(false, "code_health_review");
    let names = tool_names(&server);
    assert_has_config_tools(&names);
}

#[test]
fn enabled_tools_combines_with_standalone_filtering() {
    let _lock = config::lock_test_env();
    std::env::remove_var("CS_ENABLED_TOOLS");
    // In standalone mode, API_ONLY_TOOLS are removed first,
    // then enabled_tools further restricts the list
    let server = make_server_with_enabled_tools(true, "code_health_review,select_project");
    let names = tool_names(&server);
    // select_project is api-only, so removed in standalone even if in enabled_tools
    assert!(!names.contains(&"select_project".to_string()));
    assert!(names.contains(&"code_health_review".to_string()));
    assert_has_config_tools(&names);
}

#[test]
fn enabled_tools_single_tool() {
    let _lock = config::lock_test_env();
    std::env::remove_var("CS_ENABLED_TOOLS");
    let server = make_server_with_enabled_tools(false, "analyze_change_set");
    let names = tool_names(&server);
    assert_tool_count_and_config(&names, 6);
    assert!(names.contains(&"analyze_change_set".to_string()));
}

#[test]
fn remove_docker_unsupported_tools_drops_login_and_switch_account_routes() {
    let mut router = CodeSceneServer::tool_router();
    assert!(
        router.list_all().iter().any(|t| t.name == "login"),
        "login should be present before removal"
    );
    assert!(
        router.list_all().iter().any(|t| t.name == "switch_account"),
        "switch_account should be present before removal"
    );
    remove_docker_unsupported_tools(&mut router);
    assert!(
        !router.list_all().iter().any(|t| t.name == "login"),
        "login should be removed for Docker"
    );
    assert!(
        !router.list_all().iter().any(|t| t.name == "switch_account"),
        "switch_account should be removed for Docker"
    );
}

#[test]
fn docker_mode_removes_login_and_switch_account_tools_at_construction() {
    let _lock = config::lock_test_env();
    let _docker = environment::force_docker(true);
    let server = make_server(false);
    let names = tool_names(&server);
    assert!(
        !names.contains(&"login".to_string()),
        "login must be absent in Docker mode"
    );
    assert!(
        !names.contains(&"switch_account".to_string()),
        "switch_account must be absent in Docker mode"
    );
    assert!(
        names.contains(&"logout".to_string()),
        "logout must remain available in Docker"
    );
    assert!(names.contains(&"get_config".to_string()));
    assert!(names.contains(&"set_config".to_string()));
}
