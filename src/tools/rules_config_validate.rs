//! `rules_config_validate` — validate a Code Health rules configuration file.

use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::event_properties;
use crate::tools::common::tool_error;
use crate::tools::rules_config::{Invocation, Subcommand};
use crate::tools::RulesConfigValidateParam;
use crate::CodeSceneServer;

const TOOL: &str = "rules-config-validate";

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: RulesConfigValidateParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();

    let invocation = match build_invocation(&params) {
        Ok(inv) => inv,
        Err(msg) => {
            server.track_err_msg(TOOL, "invalid_input", &msg);
            return Ok(tool_error(&msg));
        }
    };

    match invocation.run(&*server.cli_runner).await {
        Ok(output) => {
            let props = event_properties::rules_config_properties(
                "validate",
                params.config_path.as_deref().map(Path::new),
            );
            server.track(TOOL, props);
            let text = server.maybe_version_warning(&output).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err(TOOL, &e);
            Ok(tool_error(format!("Error: {e}")))
        }
    }
}

fn build_invocation(params: &RulesConfigValidateParam) -> Result<Invocation, String> {
    Invocation::new(Subcommand::Validate)
        .config_path(params.config_path.as_deref())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_success_contains, make_cli_mock_server, set_token,
        MockCliRunner,
    };
    use crate::tools::RulesConfigValidateParam;

    fn params(config_path: Option<&str>) -> RulesConfigValidateParam {
        RulesConfigValidateParam {
            config_path: config_path.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn success_returns_validation_summary() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(
            r#"{"status":"ok","summary":"Code health config is valid: 1 rule set(s)."}"#,
        ));
        let result = server
            .rules_config_validate(Parameters(params(None)))
            .await
            .unwrap();
        assert_success_contains(&result, "is valid");
    }

    #[tokio::test]
    async fn works_without_token() {
        // Local CLI operation: no access token required.
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"status":"ok"}"#));
        let result = server
            .rules_config_validate(Parameters(params(None)))
            .await
            .unwrap();
        assert_success_contains(&result, "ok");
    }

    #[tokio::test]
    async fn relative_config_path_is_rejected() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok("unused"));
        let result = server
            .rules_config_validate(Parameters(params(Some("relative/rules.json"))))
            .await
            .unwrap();
        assert_error_contains(&result, "absolute path");
    }

    #[tokio::test]
    async fn cli_error_is_surfaced() {
        let _g = set_token("tok");
        let server =
            make_cli_mock_server(MockCliRunner::with_err(1, "No configuration file found"));
        let result = server
            .rules_config_validate(Parameters(params(None)))
            .await
            .unwrap();
        assert_error_contains(&result, "No configuration file found");
    }
}
