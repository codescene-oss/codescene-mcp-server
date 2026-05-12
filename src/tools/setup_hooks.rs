use std::fs;
use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::{Map, Value};

use crate::tools::common::tool_error;
use crate::tools::SetupHooksParam;
use crate::CodeSceneServer;

const DEFAULT_AGENT: &str = "claude-code";
const DEFAULT_SERVER_NAME: &str = "codescene";

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: SetupHooksParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    server.track("setup-hooks", serde_json::json!({}));

    let agent = params
        .agent
        .as_deref()
        .unwrap_or(DEFAULT_AGENT)
        .to_lowercase();
    let server_name = params
        .server_name
        .as_deref()
        .unwrap_or(DEFAULT_SERVER_NAME);
    let project_dir = Path::new(&params.project_dir);

    if !project_dir.is_dir() {
        return Ok(tool_error(&format!(
            "Project directory does not exist: {}",
            params.project_dir
        )));
    }

    match agent.as_str() {
        "claude-code" => install_claude_code_hooks(project_dir, server_name),
        "opencode" | "cursor" | "copilot" => Ok(unsupported_agent_response(&agent)),
        _ => Ok(tool_error(&format!(
            "Unknown agent \"{agent}\". Supported: claude-code. \
             Placeholders: opencode, cursor, copilot."
        ))),
    }
}

fn install_claude_code_hooks(
    project_dir: &Path,
    server_name: &str,
) -> Result<CallToolResult, ErrorData> {
    let claude_dir = project_dir.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    let mut settings = load_existing_settings(&settings_path);
    let our_hooks = build_hooks_object(server_name);
    merge_hooks(&mut settings, &our_hooks);

    if let Err(e) = fs::create_dir_all(&claude_dir) {
        return Ok(tool_error(&format!(
            "Failed to create .claude directory: {e}"
        )));
    }

    let json_str = serde_json::to_string_pretty(&settings).unwrap_or_default();
    if let Err(e) = fs::write(&settings_path, &json_str) {
        return Ok(tool_error(&format!(
            "Failed to write {}: {e}",
            settings_path.display()
        )));
    }

    let msg = format!(
        "Successfully installed Code Health hooks for Claude Code.\n\n\
         Written to: {path}\n\n\
         Hooks installed:\n  \
         - PostToolUse (Write|Edit): Runs code_health_review after every file change\n  \
         - PreToolUse (Bash): Runs pre_commit_code_health_safeguard before git commits\n\n\
         IMPORTANT: Restart or refresh your agent session to activate the hooks.\n\n\
         Configuration:\n  \
         - To disable commit blocking: \
         set_config key=\"hooks_block_on_regression\" value=\"false\"\n  \
         - Server name used: \"{server}\" (override with server_name parameter)",
        path = settings_path.display(),
        server = server_name,
    );
    Ok(CallToolResult::success(vec![Content::text(msg)]))
}

fn load_existing_settings(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(Map::new()))
}

fn build_hooks_object(server_name: &str) -> Value {
    serde_json::json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Write|Edit",
                    "hooks": [
                        {
                            "type": "mcp_tool",
                            "server": server_name,
                            "tool": "code_health_review",
                            "input": { "file_path": "${tool_input.file_path}" },
                            "statusMessage": "Running Code Health review..."
                        }
                    ]
                }
            ],
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {
                            "type": "mcp_tool",
                            "if": "Bash(git commit*)",
                            "server": server_name,
                            "tool": "pre_commit_code_health_safeguard",
                            "input": { "git_repository_path": "${cwd}" },
                            "statusMessage": "Running Code Health safeguard before commit..."
                        }
                    ]
                }
            ]
        }
    })
}

/// Merge our hook groups into existing settings without overwriting.
/// Appends our hook groups to existing event arrays, skipping duplicates.
fn merge_hooks(settings: &mut Value, our_config: &Value) {
    let Some(our_hooks) = our_config.get("hooks").and_then(Value::as_object) else {
        return;
    };
    let Some(settings_obj) = settings.as_object_mut() else {
        return;
    };

    let hooks_entry = settings_obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));

    let Some(existing_hooks) = hooks_entry.as_object_mut() else {
        return;
    };

    for (event_name, our_groups) in our_hooks {
        append_hook_groups(existing_hooks, event_name, our_groups);
    }
}

/// Append hook groups for a single event, skipping duplicates.
fn append_hook_groups(existing: &mut Map<String, Value>, event: &str, groups: &Value) {
    let Some(new_items) = groups.as_array() else {
        return;
    };
    let arr = existing
        .entry(event)
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(target) = arr.as_array_mut() else {
        return;
    };
    for item in new_items {
        if !target.contains(item) {
            target.push(item.clone());
        }
    }
}

fn unsupported_agent_response(agent: &str) -> CallToolResult {
    let msg = format!(
        "Hooks are not yet supported for \"{agent}\".\n\n\
         {agent} does not currently provide a hook mechanism for deterministic \
         tool invocation. This tool will be updated when support becomes available.\n\n\
         In the meantime, the AGENTS.md file and skills in this repository guide \
         the agent to use Code Health tools.\n\n\
         Currently supported agents: claude-code"
    );
    CallToolResult::success(vec![Content::text(msg)])
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;
    use tempfile::tempdir;

    use crate::tests::{make_server, result_text};
    use crate::tools::SetupHooksParam;

    fn make_params(dir: &str, agent: Option<&str>, server: Option<&str>) -> SetupHooksParam {
        SetupHooksParam {
            project_dir: dir.to_string(),
            agent: agent.map(String::from),
            server_name: server.map(String::from),
        }
    }

    async fn run_setup(
        dir: &str,
        agent: Option<&str>,
        server: Option<&str>,
    ) -> String {
        let params = make_params(dir, agent, server);
        let result = make_server(false)
            .setup_hooks(Parameters(params))
            .await
            .unwrap();
        result_text(&result).to_string()
    }

    #[tokio::test]
    async fn creates_claude_settings_in_empty_dir() {
        let tmp = tempdir().unwrap();
        let text = run_setup(tmp.path().to_str().unwrap(), None, None).await;
        assert!(text.contains("Successfully installed"));

        let settings_path = tmp.path().join(".claude/settings.json");
        assert!(settings_path.exists());

        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(content["hooks"]["PostToolUse"].is_array());
        assert!(content["hooks"]["PreToolUse"].is_array());
    }

    #[tokio::test]
    async fn merges_with_existing_settings() {
        let tmp = tempdir().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.json"),
            r#"{"permissions":{"allow":["Read"]},"hooks":{"PostToolUse":[{"matcher":"Lint","hooks":[]}]}}"#,
        )
        .unwrap();

        let text = run_setup(tmp.path().to_str().unwrap(), Some("claude-code"), None).await;
        assert!(text.contains("Successfully installed"));

        let content: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(claude_dir.join("settings.json")).unwrap(),
        )
        .unwrap();
        assert!(content["permissions"]["allow"].is_array());
        let post_arr = content["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_arr.len(), 2);
        assert_eq!(post_arr[0]["matcher"], "Lint");
        assert_eq!(post_arr[1]["matcher"], "Write|Edit");
        assert!(content["hooks"]["PreToolUse"].is_array());
    }

    #[tokio::test]
    async fn skips_duplicate_hooks() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();

        run_setup(dir, None, None).await;
        run_setup(dir, None, None).await;

        let content: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let post_arr = content["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post_arr.len(), 1, "should not duplicate hook groups");
    }

    #[tokio::test]
    async fn unsupported_agent_returns_info() {
        let tmp = tempdir().unwrap();
        let text = run_setup(tmp.path().to_str().unwrap(), Some("opencode"), None).await;
        assert!(text.contains("not yet supported"));
        assert!(text.contains("opencode"));
    }

    #[tokio::test]
    async fn defaults_to_claude_code() {
        let tmp = tempdir().unwrap();
        let text = run_setup(tmp.path().to_str().unwrap(), None, None).await;
        assert!(text.contains("Claude Code"));
        assert!(tmp.path().join(".claude/settings.json").exists());
    }

    #[tokio::test]
    async fn uses_custom_server_name() {
        let tmp = tempdir().unwrap();
        run_setup(tmp.path().to_str().unwrap(), Some("claude-code"), Some("my-server")).await;

        let content: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let server_ref = content["hooks"]["PostToolUse"][0]["hooks"][0]["server"]
            .as_str()
            .unwrap();
        assert_eq!(server_ref, "my-server");
    }

    #[tokio::test]
    async fn invalid_project_dir_returns_error() {
        let text = run_setup("/nonexistent/path/xyz", None, None).await;
        assert!(text.contains("does not exist"));
    }

    #[tokio::test]
    async fn unknown_agent_returns_error() {
        let tmp = tempdir().unwrap();
        let text = run_setup(tmp.path().to_str().unwrap(), Some("vscode"), None).await;
        assert!(text.contains("Unknown agent"));
    }
}
