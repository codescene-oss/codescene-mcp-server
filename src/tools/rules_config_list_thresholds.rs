//! `rules_config_list_thresholds` — list a language's default Code Health
//! thresholds.

use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::event_properties;
use crate::tools::common::tool_error;
use crate::tools::rules_config::{Invocation, Subcommand};
use crate::tools::RulesConfigListThresholdsParam;
use crate::CodeSceneServer;

const TOOL: &str = "rules-config-list-thresholds";

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: RulesConfigListThresholdsParam,
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
                "list-thresholds",
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

fn build_invocation(params: &RulesConfigListThresholdsParam) -> Result<Invocation, String> {
    Invocation::new(Subcommand::ListThresholds)
        .option("--language", &params.language)
        .and_then(|inv| inv.config_path(params.config_path.as_deref()))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_success_contains, make_cli_mock_server, set_token,
        MockCliRunner,
    };
    use crate::tools::RulesConfigListThresholdsParam;

    fn params(language: &str, config_path: Option<&str>) -> RulesConfigListThresholdsParam {
        RulesConfigListThresholdsParam {
            language: language.to_string(),
            config_path: config_path.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn success_returns_thresholds() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(
            r#"{"app_code":{"thresholds":[{"name":"function_max_arguments","value":4}]}}"#,
        ));
        let result = server
            .rules_config_list_thresholds(Parameters(params("Python", None)))
            .await
            .unwrap();
        assert_success_contains(&result, "function_max_arguments");
    }

    #[tokio::test]
    async fn unknown_language_surfaces_cli_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "Unknown language 'Klingon'"));
        let result = server
            .rules_config_list_thresholds(Parameters(params("Klingon", None)))
            .await
            .unwrap();
        assert_error_contains(&result, "Unknown language");
    }

    #[tokio::test]
    async fn flag_like_language_is_rejected() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok("unused"));
        let result = server
            .rules_config_list_thresholds(Parameters(params("--evil", None)))
            .await
            .unwrap();
        assert_error_contains(&result, "must not start with '-'");
    }

    #[tokio::test]
    async fn relative_config_path_is_rejected() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok("unused"));
        let result = server
            .rules_config_list_thresholds(Parameters(params("Java", Some("rel/rules.json"))))
            .await
            .unwrap();
        assert_error_contains(&result, "absolute path");
    }
}
