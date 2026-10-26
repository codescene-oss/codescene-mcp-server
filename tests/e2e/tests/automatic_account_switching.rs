//! Integration coverage for CS-12412: automatic OAuth account switching.
//!
//! A repository's `projects` metadata identifies candidate accounts. Keep a
//! matching current account, switch only to a unique target, and never switch a
//! PAT. Legacy mappings without account metadata must remain usable.
//!
//! Switching must happen before a file tool runs, even with tracking disabled.
//! The fake CLI models stored credential slots and interactive login; no browser
//! or live CodeScene credentials are required. Docker skips these host binaries.

use super::fake_http_server::{CapturedRequest, FakeHttpServer};
use super::oauth_login::{make_fake_cli, read_config, seed_config, signed_in_json_with_account};
use super::*;
use std::path::PathBuf;
use std::process::Command;

const SCORE_TOOL: &str = "code_health_score";
const HOTSPOTS_TOOL: &str = "list_technical_debt_hotspots_for_project";
const TIMEOUT: Duration = Duration::from_secs(60);
const MAPPING_PATH: &str = "/api/v2/mcp/repository-projects";
const CURRENT_TOKEN: &str = "automatic-switch-account-1";
const TARGET_TOKEN: &str = "automatic-switch-account-3";
const SIGNED_OUT: &str = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;

struct AccountSwitchEnv {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: PathBuf,
    config_dir: PathBuf,
    context_log: PathBuf,
    api: FakeHttpServer,
    _tmp: tempfile::TempDir,
}

impl AccountSwitchEnv {
    fn new(mapping: serde_json::Value) -> Self {
        let (command, mut env, repo_dir, tmp) = setup();
        let result = Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:acme/web.git"])
            .current_dir(&repo_dir)
            .output()
            .expect("add repository remote");
        assert!(result.status.success(), "git remote add failed: {result:?}");

        let api = mapping_server(mapping);
        let config_dir = tmp.path().join("config");
        std::fs::create_dir(&config_dir).expect("create isolated config");
        seed_config(
            &config_dir,
            json!({
                "instance_id": "automatic-account-switching-e2e",
                "oauth_token": CURRENT_TOKEN,
                "oauth_expires_at": "4102444800",
                "oauth_account_id": "1",
                "account_id": "1",
                "onprem_url": api.url(),
            }),
        );
        let cli = make_fake_cli(tmp.path());
        let context_log = tmp.path().join("cli-context.log");
        env.retain(|(key, _)| {
            (!key.starts_with("CS_") || key == "CS_MCP_DOWNLOAD_BASE_URL")
                && !key.starts_with("FAKE_AUTH_")
                && !key.starts_with("FAKE_CALL_")
        });
        let mut fixture = Self {
            command,
            env,
            repo_dir,
            config_dir,
            context_log,
            api,
            _tmp: tmp,
        };
        for (key, value) in [
            (
                "CS_CONFIG_DIR",
                fixture.config_dir.to_string_lossy().into_owned(),
            ),
            ("CS_CLI_PATH", cli.to_string_lossy().into_owned()),
            ("CS_DISABLE_TRACKING", "1".to_string()),
            ("CS_DISABLE_VERSION_CHECK", "1".to_string()),
            ("CS_DISABLE_SETUP_HINT", "1".to_string()),
            (
                "FAKE_CONTEXT_LOG",
                fixture.context_log.to_string_lossy().into_owned(),
            ),
            ("FAKE_AUTH_TOKEN_RESPONSE", fixture.target_session()),
        ] {
            fixture.set_env(key, value);
        }
        fixture
    }

    fn set_env(&mut self, key: &str, value: String) {
        self.env.retain(|(name, _)| name != key);
        self.env.push((key.to_string(), value));
    }

    fn target_session(&self) -> String {
        let mut response: serde_json::Value =
            serde_json::from_str(&signed_in_json_with_account(TARGET_TOKEN, 3600, 3))
                .expect("parse target session");
        response["api-url"] = json!(format!("{}/api", self.api.url()));
        response.to_string()
    }

    fn start(&self) -> MCPClient {
        let mut client = make_client(&self.command, &self.env, &self.repo_dir);
        assert!(client.start(), "Server should start");
        client.initialize().expect("Initialize should succeed");
        client
    }

    fn score(&self, client: &mut MCPClient) {
        let response = client
            .call_tool(
                SCORE_TOOL,
                json!({"file_path": self.repo_dir.join("src/utils/calculator.py").to_string_lossy()}),
                TIMEOUT,
            )
            .expect("score tool should respond");
        let text = extract_result_text(&response);
        assert_eq!(extract_code_health_score(&text), Some(9.5), "{text}");
    }

    fn cli_calls(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(&self.context_log)
            .expect("read CLI invocation context")
            .lines()
            .map(|line| {
                let (command, account) = line.rsplit_once('|').expect("CLI context fields");
                (command.to_string(), account.to_string())
            })
            .collect()
    }

    fn assert_session(&self, account: &str, token: &str) {
        let config = read_config(&self.config_dir);
        assert_eq!(
            config["oauth_account_id"], account,
            "OAuth account mismatch"
        );
        assert_eq!(config["account_id"], account, "CLI slot mismatch");
        assert_eq!(config["oauth_token"], token, "OAuth token mismatch");
        let calls = self.cli_calls();
        assert!(
            calls
                .iter()
                .any(|(command, slot)| command.starts_with("review ") && slot == account),
            "Review must run with account {account}, got {calls:?}"
        );
    }

    fn assert_target_api(&self, client: &mut MCPClient) {
        let response = client
            .call_tool(HOTSPOTS_TOOL, json!({"project_id": 10}), TIMEOUT)
            .expect("project tool should respond");
        let text = extract_result_text(&response);
        let payload: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"));
        assert_eq!(payload["data"][0]["file"], "target-account.py", "{text}");
        let requests = self.api.get_requests();
        assert!(
            requests
                .iter()
                .filter(|request| request.path.starts_with("/api/v2/projects/10/"))
                .all(|request| bearer_token(request) == Some(TARGET_TOKEN)),
            "Project API requests must use the switched OAuth token"
        );
    }
}

fn bearer_token(request: &CapturedRequest) -> Option<&str> {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .and_then(|(_, value)| value.strip_prefix("Bearer "))
}

fn mapping_server(mapping: serde_json::Value) -> FakeHttpServer {
    FakeHttpServer::start(move |request| {
        let path = request.path.split('?').next().expect("request path");
        if request.method == "GET" && path == MAPPING_PATH {
            return (200, mapping.to_string());
        }
        if path.starts_with("/api/v2/projects/10/") {
            if bearer_token(request) != Some(TARGET_TOKEN) {
                return (403, json!({"error": "wrong_account"}).to_string());
            }
            let response = if path.ends_with("/technical-debt") {
                json!({"result": [{"file": "target-account.py", "score": 3.5}], "page": 1, "max_pages": 1})
            } else {
                json!({"id": 5000})
            };
            return (200, response.to_string());
        }
        (404, json!({"error": "not_found"}).to_string())
    })
}

fn mapping(projects: serde_json::Value) -> serde_json::Value {
    let project_ids: Vec<_> = projects
        .as_array()
        .expect("projects array")
        .iter()
        .map(|project| project["project_id"].clone())
        .collect();
    json!({"repositories": [{
        "repository_id": "github.com/acme/web",
        "project_ids": project_ids,
        "projects": projects,
    }, {
        "repository_id": "github.com/unrelated/repo",
        "project_ids": [99],
        "projects": [{"project_id": 99, "account_id": 1}],
    }]})
}

fn target_mapping() -> serde_json::Value {
    mapping(json!([
        {"project_id": 10, "account_id": 3},
        {"project_id": 11, "account_id": 3},
    ]))
}

fn assert_switch(interactive: bool, restart: bool) {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let mut fixture = AccountSwitchEnv::new(target_mapping());
    if interactive {
        fixture.set_env("FAKE_AUTH_TOKEN_RESPONSE", SIGNED_OUT.to_string());
        fixture.set_env("FAKE_AUTH_LOGIN_RESPONSE", fixture.target_session());
    }
    let mut client = fixture.start();
    fixture.score(&mut client);
    fixture.assert_session("3", TARGET_TOKEN);
    assert!(
        fixture
            .api
            .get_requests()
            .iter()
            .any(|request| request.path == MAPPING_PATH),
        "Automatic switching must consult repository mappings"
    );
    let calls = fixture.cli_calls();
    let auth: Vec<_> = calls
        .iter()
        .filter(|(command, _)| command.starts_with("auth "))
        .collect();
    assert!(
        auth.iter()
            .any(|(command, slot)| command.starts_with("auth token ") && slot == "3"),
        "Must try the target credential slot: {auth:?}"
    );
    assert!(
        auth.iter().all(|(_, slot)| slot == "3"),
        "Wrong auth slot: {auth:?}"
    );
    assert_eq!(
        auth.iter()
            .any(|(command, _)| command.starts_with("auth login ")),
        interactive,
        "Interactive login should only occur for a missing stored session: {auth:?}"
    );
    fixture.assert_target_api(&mut client);

    if restart {
        drop(client);
        let before = fixture.cli_calls();
        let mut client = fixture.start();
        fixture.score(&mut client);
        fixture.assert_session("3", TARGET_TOKEN);
        let after = fixture.cli_calls();
        assert!(
            after[before.len()..]
                .iter()
                .all(|(command, _)| !command.starts_with("auth ")),
            "A restarted server should reuse the persisted session"
        );
        fixture.assert_target_api(&mut client);
    }
}

#[test]
pub fn test_automatic_account_switching_reuses_stored_session() {
    assert_switch(false, false);
}

#[test]
pub fn test_automatic_account_switching_logs_in_when_slot_missing() {
    assert_switch(true, false);
}

#[test]
pub fn test_automatic_account_switching_persists_across_restart() {
    assert_switch(false, true);
}

#[test]
pub fn test_automatic_account_switching_fixture_manual_control() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    for interactive in [false, true] {
        let mut fixture = AccountSwitchEnv::new(target_mapping());
        if interactive {
            fixture.set_env("FAKE_AUTH_TOKEN_RESPONSE", SIGNED_OUT.to_string());
            fixture.set_env("FAKE_AUTH_LOGIN_RESPONSE", fixture.target_session());
        }
        let mut client = fixture.start();
        let response = client
            .call_tool("switch_account", json!({"account_id": 3}), TIMEOUT)
            .expect("manual fixture control should respond");
        assert!(
            extract_result_text(&response).contains("Switched to CodeScene account 3"),
            "Manual fixture control failed: {response}"
        );
        fixture.score(&mut client);
        fixture.assert_session("3", TARGET_TOKEN);
        fixture.assert_target_api(&mut client);
        drop(client);
        let before = fixture.cli_calls();
        let mut client = fixture.start();
        fixture.score(&mut client);
        fixture.assert_session("3", TARGET_TOKEN);
        fixture.assert_target_api(&mut client);
        assert!(
            fixture.cli_calls()[before.len()..]
                .iter()
                .all(|(command, _)| !command.starts_with("auth ")),
            "Manual control must reuse the persisted session after restart"
        );
    }
}

fn assert_no_switch(mapping: serde_json::Value, pat: bool) {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let mut fixture = AccountSwitchEnv::new(mapping);
    if pat {
        fixture.set_env("CS_ACCESS_TOKEN", "automatic-switch-pat".to_string());
    }
    let mut client = fixture.start();
    fixture.score(&mut client);
    fixture.assert_session("1", CURRENT_TOKEN);
    let calls = fixture.cli_calls();
    assert!(
        calls
            .iter()
            .all(|(command, _)| !command.starts_with("auth ")),
        "Must not reauthenticate or select another account: {calls:?}"
    );
}

#[test]
pub fn test_automatic_account_switching_keeps_matching_account() {
    assert_no_switch(
        mapping(json!([
            {"project_id": 10, "account_id": 3},
            {"project_id": 2, "account_id": 1},
        ])),
        false,
    );
}

#[test]
pub fn test_automatic_account_switching_keeps_ambiguous_account() {
    assert_no_switch(
        mapping(json!([
            {"project_id": 10, "account_id": 3},
            {"project_id": 20, "account_id": 4},
        ])),
        false,
    );
}

#[test]
pub fn test_automatic_account_switching_never_switches_pat() {
    assert_no_switch(target_mapping(), true);
}

#[test]
pub fn test_automatic_account_switching_ignores_unmatched_repository() {
    let mut response = target_mapping();
    response["repositories"][0]["repository_id"] = json!("github.com/other/repo");
    assert_no_switch(response, false);
}

#[test]
pub fn test_automatic_account_switching_supports_legacy_mapping() {
    assert_no_switch(
        json!({"repositories": [{
            "repository_id": "github.com/acme/web", "project_ids": [10],
        }]}),
        false,
    );
}

#[test]
pub fn test_automatic_account_switching_before_project_api() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let fixture = AccountSwitchEnv::new(mapping(json!([
        {"project_id": 2, "account_id": 1},
        {"project_id": 10, "account_id": 3},
    ])));
    let mut client = fixture.start();
    fixture.assert_target_api(&mut client);
    fixture.score(&mut client);
    fixture.assert_session("3", TARGET_TOKEN);
    assert!(
        fixture.api.get_requests().iter().any(|request| {
            request.path == MAPPING_PATH && bearer_token(request) == Some(CURRENT_TOKEN)
        }),
        "Mapping discovery must use the original account's credential"
    );
}

#[test]
pub fn test_automatic_account_switching_uses_file_repository() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let fixture = AccountSwitchEnv::new(target_mapping());
    let mut client = make_client(&fixture.command, &fixture.env, fixture._tmp.path());
    assert!(client.start(), "Server should start outside the repository");
    client.initialize().expect("Initialize should succeed");
    fixture.score(&mut client);
    fixture.assert_session("3", TARGET_TOKEN);
    fixture.assert_target_api(&mut client);
}

#[test]
pub fn test_automatic_account_switching_blocks_tool_after_failed_switch() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    for token_response in [
        SIGNED_OUT.to_string(),
        signed_in_json_with_account("wrong-account", 3600, 4),
    ] {
        let mut fixture = AccountSwitchEnv::new(target_mapping());
        fixture.set_env("FAKE_AUTH_TOKEN_RESPONSE", token_response);
        fixture.set_env("FAKE_AUTH_LOGIN_RESPONSE", SIGNED_OUT.to_string());
        let mut client = fixture.start();
        let response = client.call_tool(
            SCORE_TOOL,
            json!({"file_path": fixture.repo_dir.join("src/utils/calculator.py").to_string_lossy()}),
            TIMEOUT,
        ).expect("failed-switch tool should respond");
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert!(
            extract_result_text(&response).contains("Automatic CodeScene account switching failed"),
            "The switch failure must be reported: {response}"
        );
        assert!(
            fixture
                .cli_calls()
                .iter()
                .all(|(command, _)| !command.starts_with("review ")),
            "Review must not run after failed authentication"
        );
    }
}

#[test]
pub fn test_automatic_account_switching_keeps_session_for_invalid_mapping() {
    assert_no_switch(json!({"repositories": "invalid"}), false);
}
