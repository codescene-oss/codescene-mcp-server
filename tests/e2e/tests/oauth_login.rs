//! OAuth login end-to-end tests.
//!
//! The real OAuth authorization-code flow (browser redirect + provider
//! token issuance) cannot be faked in an e2e test. Instead, these tests
//! fake the embedded CLI (via `CS_CLI_PATH`), which is the actual contract
//! the MCP server depends on: `cs auth token --client mcp --output-format
//! json` and `cs auth login --client mcp --output-format json`.
//!
//! This lets us verify MCP-side behavior that previously regressed:
//! - OAuth token persistence into the config-backed environment/config file
//! - Falling back to `auth token` when a login response omits `access-token`
//! - Reuse of persisted OAuth state across a fresh MCP server process
//! - Refresh behavior when expiry exists but the token itself is missing
//! - Correct precedence between a configured PAT and OAuth state
//!
//! Every test gets its own temp `CS_CONFIG_DIR` (never the real user config
//! dir) and its own temp marker/log files for the fake CLI, so there is no
//! shared state between tests or processes.

use super::*;
use std::process::Command;

const SIGNED_OUT_JSON: &str =
    r#"{"status":"signed_out","access-token":null,"api-url":null}"#;

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn signed_in_json(token: &str, expires_in_secs: i64) -> String {
    let expires_at = now_epoch() + expires_in_secs;
    format!(
        r#"{{"status":"signed_in","access-token":"{token}","api-url":"https://api.codescene.io/api","expires-at":{expires_at},"refresh-token-expires-at":{}}}"#,
        expires_at + 3600
    )
}

fn signed_in_without_access_token(expires_in_secs: i64) -> String {
    let expires_at = now_epoch() + expires_in_secs;
    format!(
        r#"{{"status":"signed_in","access-token":null,"api-url":"https://api.codescene.io/api","expires-at":{expires_at},"refresh-token-expires-at":{}}}"#,
        expires_at + 3600
    )
}

// ---------------------------------------------------------------------------
// Fake CLI
// ---------------------------------------------------------------------------

/// Fake `cs` CLI that answers `auth token`, `auth login`, and `review`.
///
/// Responses are controlled entirely via env vars so a single compiled
/// binary can be reused across every test in this file:
/// - `FAKE_AUTH_TOKEN_RESPONSE` — response for the first `auth token` call
/// - `FAKE_AUTH_TOKEN_RESPONSE_2` — response for subsequent `auth token`
///   calls (falls back to `FAKE_AUTH_TOKEN_RESPONSE` if unset)
/// - `FAKE_AUTH_LOGIN_RESPONSE` — response for `auth login` calls
/// - `FAKE_CALL_MARKER` — path to a marker file used to distinguish the
///   first vs. later `auth token` call
/// - `FAKE_CALL_LOG` — path to an append-only log of every `auth ...`
///   invocation, used to assert the CLI auth endpoints were/weren't called
const FAKE_CLI_RS: &str = r##"use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).filter(|a| !a.starts_with("-D")).collect();

    if args.first().map(|s| s.as_str()) == Some("auth") {
        if let Ok(log_path) = env::var("FAKE_CALL_LOG") {
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(f, "{}", args.join(" "));
            }
        }
    }

    match (args.first().map(|s| s.as_str()), args.get(1).map(|s| s.as_str())) {
        (Some("auth"), Some("token")) => {
            let marker = env::var("FAKE_CALL_MARKER").ok();
            let already_called = marker
                .as_deref()
                .map(|p| Path::new(p).is_file())
                .unwrap_or(false);
            if let Some(p) = &marker {
                let _ = fs::write(p, "1");
            }
            let resp = if already_called {
                env::var("FAKE_AUTH_TOKEN_RESPONSE_2")
                    .or_else(|_| env::var("FAKE_AUTH_TOKEN_RESPONSE"))
            } else {
                env::var("FAKE_AUTH_TOKEN_RESPONSE")
            }
            .unwrap_or_else(|_| {
                r#"{"status":"signed_out","access-token":null,"api-url":null}"#.to_string()
            });
            println!("{resp}");
            process::exit(0);
        }
        (Some("auth"), Some("login")) => {
            let resp = env::var("FAKE_AUTH_LOGIN_RESPONSE").unwrap_or_else(|_| {
                r#"{"status":"signed_out","access-token":null,"api-url":null}"#.to_string()
            });
            println!("{resp}");
            process::exit(0);
        }
        (Some("review"), _) => {
            println!(r#"{{"score":9.5,"review":[]}}"#);
            process::exit(0);
        }
        (Some("version"), _) => {
            println!("fake-cli-version");
            process::exit(0);
        }
        _ => {
            eprintln!("fake-cli: unsupported command: {}", args.join(" "));
            process::exit(1);
        }
    }
}
"##;

/// Compile the fake CLI from embedded Rust source and return its path.
fn make_fake_cli(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("fake_cs_oauth.rs");
    std::fs::write(&source, FAKE_CLI_RS).expect("write fake CLI source");

    let binary_name = if cfg!(windows) { "cs.exe" } else { "cs" };
    let output_path = dir.join(binary_name);

    let result = Command::new("rustc")
        .args([
            source.to_str().expect("source path"),
            "-O",
            "-o",
            output_path.to_str().expect("output path"),
        ])
        .output()
        .expect("rustc should execute");

    assert!(
        result.status.success(),
        "Failed to compile fake CLI: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    output_path
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct OAuthTestEnv {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: std::path::PathBuf,
    config_dir: std::path::PathBuf,
    call_log_path: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

/// Build a fresh MCP environment with a fake CLI, a temp `CS_CONFIG_DIR`,
/// and no `CS_ACCESS_TOKEN` (unless `extra_env` sets one). Every call gets
/// its own temp dir, so marker/log files never leak across tests.
fn oauth_setup(extra_env: &[(&str, &str)]) -> OAuthTestEnv {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_oauth_").expect("create temp dir");
    let sample_files = get_sample_files();
    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("create git repo");

    let fake_cli = make_fake_cli(temp_dir.path());
    let config_dir = temp_dir.path().join(".cs_config_oauth");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let call_marker = temp_dir.path().join(".auth_token_marker");
    let call_log = temp_dir.path().join(".auth_calls.log");

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let env: Vec<(String, String)> = env_map
        .into_iter()
        .filter(|(k, _)| k != "CS_ACCESS_TOKEN")
        .chain(
            [
                ("CS_CLI_PATH", fake_cli.to_string_lossy().into_owned()),
                (
                    "CS_CONFIG_DIR",
                    config_dir.to_string_lossy().into_owned(),
                ),
                ("CS_DISABLE_VERSION_CHECK", "1".to_string()),
                ("CS_DISABLE_TRACKING", "1".to_string()),
                (
                    "FAKE_CALL_MARKER",
                    call_marker.to_string_lossy().into_owned(),
                ),
                ("FAKE_CALL_LOG", call_log.to_string_lossy().into_owned()),
            ]
            .into_iter()
            .map(|(k, v): (&str, String)| (k.to_string(), v)),
        )
        .chain(
            extra_env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        )
        .collect();

    let command = backend.get_command(&repo_dir);
    OAuthTestEnv {
        command,
        env,
        repo_dir,
        config_dir,
        call_log_path: call_log,
        _tmp: temp_dir,
    }
}

fn read_config(config_dir: &Path) -> serde_json::Value {
    let path = config_dir.join("config.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read config file {}: {e}", path.display()));
    serde_json::from_str(&content).expect("parse config.json")
}

fn config_file_exists(config_dir: &Path) -> bool {
    config_dir.join("config.json").is_file()
}

fn seed_config(config_dir: &Path, values: serde_json::Value) {
    let path = config_dir.join("config.json");
    std::fs::write(&path, serde_json::to_string_pretty(&values).unwrap())
        .expect("seed config file");
}

/// Start the MCP server and complete the JSON-RPC handshake.
fn start_client(t: &OAuthTestEnv) -> MCPClient {
    let mut client = make_client(&t.command, &t.env, &t.repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

fn call_login(client: &mut MCPClient) -> String {
    let response = client
        .call_tool("login", json!({}), Duration::from_secs(30))
        .expect("login call should succeed");
    extract_result_text(&response)
}

fn call_score(client: &mut MCPClient, repo_dir: &Path) -> String {
    let file = repo_dir.join("src/utils/calculator.py");
    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": file.to_string_lossy()}),
            Duration::from_secs(30),
        )
        .expect("code_health_score call should succeed");
    extract_result_text(&response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A configured PAT must short-circuit OAuth entirely: `login` should not
/// call the CLI auth endpoints, and no OAuth state should be persisted.
pub fn test_login_skips_when_pat_configured() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let t = oauth_setup(&[("CS_ACCESS_TOKEN", "pat-token")]);
    let mut client = start_client(&t);

    let result = call_login(&mut client);
    assert!(
        result.contains("CS_ACCESS_TOKEN is already configured"),
        "got: {result}"
    );

    if config_file_exists(&t.config_dir) {
        let config = read_config(&t.config_dir);
        assert!(
            config.get("oauth_token").is_none(),
            "should not persist oauth_token when PAT is configured, got: {config}"
        );
    }

    let log = std::fs::read_to_string(&t.call_log_path).unwrap_or_default();
    assert!(
        log.is_empty(),
        "CLI auth endpoints should not be called when PAT is configured, got log: {log}"
    );
}

/// When the CLI already reports a fresh signed-in session, `login` should
/// reuse it and persist the OAuth token into the config file.
pub fn test_login_reuses_existing_session() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let token_resp = signed_in_json("session-token", 3600);
    let t = oauth_setup(&[("FAKE_AUTH_TOKEN_RESPONSE", token_resp.as_str())]);
    let mut client = start_client(&t);

    let result = call_login(&mut client);
    assert!(result.contains("Already signed in"), "got: {result}");

    let config = read_config(&t.config_dir);
    assert_eq!(config["oauth_token"].as_str(), Some("session-token"));
    assert!(config.get("oauth_expires_at").is_some());
}

/// Expectations for a login-flow scenario that exercises the full
/// `login` -> persisted config -> guarded tool call path. Shared by the
/// interactive-success and failed-login regression tests below, which
/// otherwise differ only in these values.
struct LoginFlowExpectation<'a> {
    /// Response the fake CLI returns for `auth login`.
    login_response: &'a str,
    /// Substring expected in the `login` tool result text.
    result_contains: &'a str,
    /// Expected `oauth_token` value in the persisted config, if any.
    oauth_token: Option<&'a str>,
    /// Expected `oauth_expires_at` value in the persisted config, if any.
    oauth_expires_at: Option<&'a str>,
    /// Whether a subsequent guarded tool call should succeed using the
    /// resulting auth state (`true`) or keep reporting a missing token
    /// (`false`).
    score_has_token: bool,
}

/// Runs `login` against a fresh session, then asserts on the tool result,
/// the persisted config file, and a subsequent guarded tool call.
fn assert_login_flow(expect: &LoginFlowExpectation) {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let t = oauth_setup(&[
        ("FAKE_AUTH_TOKEN_RESPONSE", SIGNED_OUT_JSON),
        ("FAKE_AUTH_LOGIN_RESPONSE", expect.login_response),
    ]);
    let mut client = start_client(&t);

    let result = call_login(&mut client);
    assert!(result.contains(expect.result_contains), "got: {result}");

    let config = read_config(&t.config_dir);
    if let Some(token) = expect.oauth_token {
        assert_eq!(config["oauth_token"].as_str(), Some(token));
    }
    if let Some(expires_at) = expect.oauth_expires_at {
        assert_eq!(config["oauth_expires_at"].as_str(), Some(expires_at));
    }

    let score = call_score(&mut client, &t.repo_dir);
    if expect.score_has_token {
        assert!(
            !score.contains("No access token configured"),
            "Guarded tool should work using the freshly persisted OAuth token, got: {score}"
        );
    } else {
        assert!(
            score.contains("No access token configured"),
            "Should keep reporting missing token, got: {score}"
        );
    }
}

/// When no session exists, `login` should run the interactive flow and
/// persist the resulting token, which a subsequent guarded tool call can use.
pub fn test_login_interactive_flow_persists_token() {
    let login_resp = signed_in_json("interactive-token", 3600);
    assert_login_flow(&LoginFlowExpectation {
        login_response: login_resp.as_str(),
        result_contains: "Successfully signed in",
        oauth_token: Some("interactive-token"),
        oauth_expires_at: None,
        score_has_token: true,
    });
}

/// Regression test: when the login response omits `access-token`, MCP must
/// immediately fetch it via `auth token` and persist the real token, not null.
pub fn test_login_fetches_token_when_login_omits_access_token() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let fetched_resp = signed_in_json("fetched-token", 3600);
    let login_without_token = signed_in_without_access_token(3600);
    let t = oauth_setup(&[
        ("FAKE_AUTH_TOKEN_RESPONSE", SIGNED_OUT_JSON),
        ("FAKE_AUTH_TOKEN_RESPONSE_2", fetched_resp.as_str()),
        ("FAKE_AUTH_LOGIN_RESPONSE", login_without_token.as_str()),
    ]);
    let mut client = start_client(&t);

    let result = call_login(&mut client);
    assert!(result.contains("Successfully signed in"), "got: {result}");

    let config = read_config(&t.config_dir);
    assert_eq!(config["oauth_token"].as_str(), Some("fetched-token"));
}

/// A login that never completes must persist the signed-out sentinel, and
/// guarded tools must keep reporting "no token" rather than crashing.
pub fn test_failed_login_persists_signed_out_state() {
    assert_login_flow(&LoginFlowExpectation {
        login_response: SIGNED_OUT_JSON,
        result_contains: "Login did not complete",
        oauth_token: None,
        oauth_expires_at: Some("0"),
        score_has_token: false,
    });
}

/// OAuth state persisted by one MCP process must be reusable by a second,
/// independently started MCP process without any further CLI auth calls.
pub fn test_persisted_oauth_reused_by_second_process() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let login_resp = signed_in_json("persisted-token", 3600);
    let t = oauth_setup(&[
        ("FAKE_AUTH_TOKEN_RESPONSE", SIGNED_OUT_JSON),
        ("FAKE_AUTH_LOGIN_RESPONSE", login_resp.as_str()),
    ]);

    {
        let mut client = start_client(&t);
        let result = call_login(&mut client);
        assert!(result.contains("Successfully signed in"), "got: {result}");
        client.stop();
    }

    let config = read_config(&t.config_dir);
    assert_eq!(config["oauth_token"].as_str(), Some("persisted-token"));

    let log_after_login = std::fs::read_to_string(&t.call_log_path).unwrap_or_default();

    let mut client2 = start_client(&t);

    let score = call_score(&mut client2, &t.repo_dir);
    assert!(
        !score.contains("No access token configured"),
        "Second process should reuse persisted OAuth token, got: {score}"
    );

    let log_after_score = std::fs::read_to_string(&t.call_log_path).unwrap_or_default();
    assert_eq!(
        log_after_login, log_after_score,
        "Second process should not need to call CLI auth endpoints again"
    );
}

/// Regression test: cached state with an expiry but no token must not be
/// treated as authenticated — MCP must refresh via the CLI and repair state.
pub fn test_expiry_without_token_triggers_refresh() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let recovered_resp = signed_in_json("recovered-token", 3600);
    let t = oauth_setup(&[("FAKE_AUTH_TOKEN_RESPONSE", recovered_resp.as_str())]);

    seed_config(
        &t.config_dir,
        json!({
            "instance_id": "test-instance-id",
            "oauth_expires_at": (now_epoch() + 3600).to_string(),
        }),
    );

    let mut client = start_client(&t);

    let score = call_score(&mut client, &t.repo_dir);
    assert!(
        !score.contains("No access token configured"),
        "Should refresh via CLI when expiry exists but token is missing, got: {score}"
    );

    let config = read_config(&t.config_dir);
    assert_eq!(config["oauth_token"].as_str(), Some("recovered-token"));
}

/// A configured PAT must take precedence over any cached OAuth state, and
/// resolving it must never invoke the CLI auth endpoints.
pub fn test_pat_takes_precedence_over_oauth() {
    if is_docker() {
        return skip_if_docker("fake CLI binary not available in container");
    }
    let should_not_be_used = signed_in_json("should-not-be-used", 3600);
    let t = oauth_setup(&[
        ("CS_ACCESS_TOKEN", "pat-token"),
        ("FAKE_AUTH_TOKEN_RESPONSE", should_not_be_used.as_str()),
    ]);

    seed_config(
        &t.config_dir,
        json!({
            "instance_id": "test-instance-id",
            "oauth_token": "stale-oauth-token",
            "oauth_expires_at": (now_epoch() + 3600).to_string(),
        }),
    );

    let mut client = start_client(&t);

    let score = call_score(&mut client, &t.repo_dir);
    assert!(
        !score.contains("No access token configured"),
        "Should succeed using the configured PAT, got: {score}"
    );

    let log = std::fs::read_to_string(&t.call_log_path).unwrap_or_default();
    assert!(
        log.is_empty(),
        "CLI auth endpoints should not be called when PAT is configured, got log: {log}"
    );
}
