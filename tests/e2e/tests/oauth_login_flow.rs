//! Host-only OAuth Authorization Code e2e against a mock CodeScene OAuth server.
//!
//! Unlike `oauth_login.rs` (which fakes the CLI JSON contract via `CS_CLI_PATH`),
//! these tests drive the **real embedded CLI**:
//!
//! 1. MCP `login` → `cs auth login --client mcp`
//! 2. CLI discovers OAuth via empty `POST /oauth2/token` on `CS_ONPREM_URL`
//! 3. CLI opens a browser to `/oauth2/auth` — but the CLI uses the OS-native
//!    opener (`open` on macOS, `xdg-open` on Linux, `cmd /c start` on Windows),
//!    which ignores `$BROWSER`. To stay headless we prepend a shim directory to
//!    `PATH` containing fake `open`/`xdg-open` executables that forward the URL
//!    to a tiny in-process HTTP helper instead of launching the real browser.
//! 4. Mock redirects to `http://127.0.0.1:19876/callback` with `code` + `state`
//! 5. CLI exchanges the code (PKCE) and returns signed-in JSON
//! 6. MCP persists `CS_OAUTH_*` into an isolated `CS_CONFIG_DIR`
//!
//! Docker is skipped: OAuth localhost callback is unsupported in containers.
//! Keep `oauth_login.rs` for fast MCP-contract coverage with a fake CLI.

use super::fake_oauth_server::{FakeOAuthServer, FakeOAuthServerOptions};
use super::*;
use std::path::{Path, PathBuf};
use std::process::Command;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);

/// Platform-specific `PATH` entry separator.
#[cfg(windows)]
const PATH_SEPARATOR: &str = ";";
#[cfg(not(windows))]
const PATH_SEPARATOR: &str = ":";

/// Tiny HTTP client that stands in for the browser: it fetches the CLI's
/// authorize URL headlessly and follows a single 302 to the localhost
/// callback. Installed on `PATH` as `open`/`xdg-open` so the CLI's native
/// browser launcher invokes it instead of a real browser.
const BROWSER_HELPER_RS: &str = r##"use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn main() {
    let url = env::args().nth(1).expect("url argument required");
    let url = url.trim().trim_matches(|c| c == '\'' || c == '"');
    eprintln!("oauth-browser-helper opening {url}");
    match fetch(url) {
        Ok(resp) => {
            eprintln!("oauth-browser-helper status {}", resp.lines().next().unwrap_or(""));
            if let Some(loc) = resp.lines().find(|l| l.to_ascii_lowercase().starts_with("location:")) {
                let loc = loc.splitn(2, ':').nth(1).unwrap_or("").trim();
                if !loc.is_empty() {
                    eprintln!("oauth-browser-helper follow {loc}");
                    let _ = fetch(loc);
                }
            }
        }
        Err(e) => {
            eprintln!("oauth-browser-helper error: {e}");
            std::process::exit(1);
        }
    }
}

fn fetch(url: &str) -> Result<String, String> {
    let without = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| format!("unsupported url: {url}"))?;
    let (hostport, path) = match without.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (without, "/".to_string()),
    };
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|e| e.to_string())?),
        None => (hostport, 80u16),
    };
    let mut stream =
        TcpStream::connect((host, port)).map_err(|e| format!("connect {hostport}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .ok();
    let req = format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    let mut buf = String::new();
    stream
        .read_to_string(&mut buf)
        .map_err(|e| format!("read: {e}"))?;
    Ok(buf)
}
"##;

struct OAuthFlowEnv {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: PathBuf,
    config_dir: PathBuf,
    server: FakeOAuthServer,
    _tmp: tempfile::TempDir,
    _cli_home: tempfile::TempDir,
}

/// Result of building the headless browser helper: the helper binary itself
/// (usable as `$BROWSER`) and a directory to prepend to `PATH` that shadows the
/// OS-native browser openers so the CLI never launches a real browser.
struct BrowserShim {
    /// The compiled helper binary. Suitable for `$BROWSER` on platforms that
    /// honor it.
    helper: PathBuf,
    /// Directory containing `open`/`xdg-open` (Unix) or `open.cmd` (Windows)
    /// shims that forward to `helper`. Prepend this to `PATH`.
    path_dir: PathBuf,
}

fn compile_browser_helper(dir: &Path) -> BrowserShim {
    let source = dir.join("oauth_browser_helper.rs");
    std::fs::write(&source, BROWSER_HELPER_RS).expect("write browser helper source");

    let binary_name = if cfg!(windows) {
        "oauth_browser_helper.exe"
    } else {
        "oauth_browser_helper"
    };
    let helper = dir.join(binary_name);

    let result = Command::new("rustc")
        .args([
            source.to_str().expect("source path"),
            "-O",
            "-o",
            helper.to_str().expect("output path"),
        ])
        .output()
        .expect("rustc should execute");

    assert!(
        result.status.success(),
        "Failed to compile browser helper: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let path_dir = dir.join("browser_shim_bin");
    std::fs::create_dir_all(&path_dir).expect("create browser shim dir");
    install_browser_shims(&path_dir, &helper);

    BrowserShim { helper, path_dir }
}

/// Install fake browser openers into `path_dir` that forward their URL argument
/// to `helper`. The CLI resolves `open`/`xdg-open`/`cmd` via `PATH`, so placing
/// these shims first keeps the OAuth flow headless.
#[cfg(not(windows))]
fn install_browser_shims(path_dir: &Path, helper: &Path) {
    use std::os::unix::fs::PermissionsExt;

    // macOS uses `open <url>`, Linux uses `xdg-open <url>`. Both receive the URL
    // as the first argument, exactly like the helper expects.
    let script = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", helper.display());
    for name in ["open", "xdg-open"] {
        let shim = path_dir.join(name);
        std::fs::write(&shim, &script).expect("write browser shim");
        let mut perms = std::fs::metadata(&shim)
            .expect("stat browser shim")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim, perms).expect("chmod browser shim");
    }
}

/// On Windows the CLI launches the browser via `cmd /c start "" "<url>"`, so
/// shadowing a single opener executable is not reliable. Install an `open.cmd`
/// shim (harmless if unused) and rely on the `$BROWSER` fallback instead.
#[cfg(windows)]
fn install_browser_shims(path_dir: &Path, helper: &Path) {
    let cmd_path = path_dir.join("open.cmd");
    let script = format!("@echo off\r\n\"{}\" %*\r\n", helper.display());
    std::fs::write(&cmd_path, script).expect("write browser shim cmd");
}

/// Build a `PATH` value with `dir` prepended to `existing`, using the
/// platform-specific separator. Falls back to just `dir` when there is no
/// existing `PATH`.
fn prepend_to_path(dir: &Path, existing: Option<&String>) -> String {
    let dir = dir.to_string_lossy();
    match existing.filter(|p| !p.is_empty()) {
        Some(path) => format!("{dir}{PATH_SEPARATOR}{path}"),
        None => dir.into_owned(),
    }
}

fn oauth_flow_setup(oauth_enabled: bool) -> OAuthFlowEnv {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_oauth_flow_").expect("create temp dir");
    let sample_files = get_sample_files();
    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("create git repo");

    let config_dir = temp_dir.path().join(".cs_config_oauth_flow");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let cli_home = tempfile::Builder::new()
        .prefix("cs_mcp_oauth_cli_home_")
        .tempdir()
        .expect("create isolated CLI home");
    // Windows CLI stores OAuth creds under %APPDATA%\Codescene\credentials.
    // Unix CLI uses $HOME/.codescene (or similar under the user home).
    let app_data = cli_home.path().join("AppData").join("Roaming");
    std::fs::create_dir_all(&app_data).expect("create isolated APPDATA");
    std::fs::create_dir_all(cli_home.path().join(".codescene")).expect("create .codescene");

    let browser = compile_browser_helper(temp_dir.path());
    let server = FakeOAuthServer::start_with_options(FakeOAuthServerOptions { oauth_enabled });

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);

    // Prepend the shim directory to PATH so the CLI's native browser opener
    // (`open`/`xdg-open`) resolves to our headless helper instead of the real
    // browser. `$BROWSER` alone is insufficient because the CLI uses the OS
    // opener, which ignores it.
    let shadowed_path = prepend_to_path(&browser.path_dir, env_map.get("PATH"));

    let env: Vec<(String, String)> = env_map
        .into_iter()
        .filter(|(k, _)| {
            k != "CS_ACCESS_TOKEN"
                && k != "CS_CLI_PATH"
                && k != "CS_CONFIG_DIR"
                && k != "CS_ONPREM_URL"
                && k != "BROWSER"
                && k != "HOME"
                && k != "APPDATA"
                && k != "PATH"
        })
        .chain(
            [
                (
                    "CS_CONFIG_DIR",
                    config_dir.to_string_lossy().into_owned(),
                ),
                ("CS_ONPREM_URL", server.url()),
                ("CS_DISABLE_VERSION_CHECK", "1".to_string()),
                ("CS_DISABLE_TRACKING", "1".to_string()),
                ("BROWSER", browser.helper.to_string_lossy().into_owned()),
                ("PATH", shadowed_path),
                ("HOME", cli_home.path().to_string_lossy().into_owned()),
                ("APPDATA", app_data.to_string_lossy().into_owned()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v)),
        )
        .collect();

    let command = backend.get_command(&repo_dir);
    OAuthFlowEnv {
        command,
        env,
        repo_dir,
        config_dir,
        server,
        _tmp: temp_dir,
        _cli_home: cli_home,
    }
}

fn start_client(t: &OAuthFlowEnv) -> MCPClient {
    let mut client = make_client(&t.command, &t.env, &t.repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

fn call_login(client: &mut MCPClient) -> String {
    let response = client
        .call_tool("login", json!({}), LOGIN_TIMEOUT)
        .expect("login call should succeed");
    extract_result_text(&response)
}

fn read_config(config_dir: &Path) -> serde_json::Value {
    let path = config_dir.join("config.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&content).expect("parse config.json")
}

/// Real CLI + mock IdP happy path: authorize → callback → token → MCP persist.
pub fn test_oauth_authorization_code_flow_persists_token() {
    if is_docker() {
        return skip_if_docker("OAuth localhost callback unsupported in Docker");
    }

    let t = oauth_flow_setup(true);
    let mut client = start_client(&t);

    let result = call_login(&mut client);
    assert!(
        result.contains("Successfully signed in"),
        "expected successful OAuth login, got: {result}"
    );

    let config = read_config(&t.config_dir);
    assert_eq!(
        config["oauth_token"].as_str(),
        Some(t.server.access_token()),
        "MCP should persist the OAuth access token, got: {config}"
    );
    assert!(
        config.get("oauth_expires_at").is_some(),
        "expected oauth_expires_at in config, got: {config}"
    );

    let log = t.server.request_log();
    assert!(
        log.discovery_posts >= 1,
        "CLI should probe /oauth2/token for discovery, log={log:?}"
    );
    assert_eq!(
        log.authorize_gets, 1,
        "expected one authorize redirect, log={log:?}"
    );
    assert_eq!(
        log.auth_code_exchanges, 1,
        "expected one authorization_code exchange, log={log:?}"
    );

    // Second login should reuse the CLI/MCP session without another authorize hit.
    let result2 = call_login(&mut client);
    assert!(
        result2.contains("Already signed in"),
        "expected session reuse, got: {result2}"
    );
    let log2 = t.server.request_log();
    assert_eq!(
        log2.authorize_gets, 1,
        "reuse must not hit /oauth2/auth again, log={log2:?}"
    );
}

/// When OAuth routes are missing, login must fail and not leave a usable token.
pub fn test_oauth_flow_fails_when_routes_missing() {
    if is_docker() {
        return skip_if_docker("OAuth localhost callback unsupported in Docker");
    }

    let t = oauth_flow_setup(false);
    let mut client = start_client(&t);

    let result = call_login(&mut client);
    assert!(
        result.contains("Login failed") || result.contains("did not complete"),
        "expected login failure when OAuth routes are missing, got: {result}"
    );

    if t.config_dir.join("config.json").is_file() {
        let config = read_config(&t.config_dir);
        let token = config.get("oauth_token").and_then(|v| v.as_str());
        assert!(
            token.is_none() || token == Some(""),
            "failed login must not persist a usable oauth_token, got: {config}"
        );
    }

    let log = t.server.request_log();
    assert_eq!(
        log.authorize_gets, 0,
        "authorize must not run when discovery fails, log={log:?}"
    );
    assert_eq!(
        log.auth_code_exchanges, 0,
        "token exchange must not run when discovery fails, log={log:?}"
    );
}
