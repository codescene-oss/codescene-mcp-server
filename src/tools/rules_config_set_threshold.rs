//! `rules_config_set_threshold` — set a Code Health threshold value.

use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::event_properties;
use crate::tools::common::tool_error;
use crate::tools::rules_config::{Invocation, Subcommand};
use crate::tools::RulesConfigSetThresholdParam;
use crate::CodeSceneServer;

const TOOL: &str = "rules-config-set-threshold";

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: RulesConfigSetThresholdParam,
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
                "set-threshold",
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

fn build_invocation(params: &RulesConfigSetThresholdParam) -> Result<Invocation, String> {
    Invocation::new(Subcommand::SetThreshold)
        .option("--threshold-name", &params.threshold_name)
        .map(|inv| inv.trusted_option("--value", params.value.to_string()))
        .and_then(|inv| {
            inv.maybe_option(
                "--matching-content-path",
                params.matching_content_path.as_deref(),
            )
        })
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
    use crate::tools::RulesConfigSetThresholdParam;

    fn params(threshold_name: &str, value: u32) -> RulesConfigSetThresholdParam {
        RulesConfigSetThresholdParam {
            threshold_name: threshold_name.to_string(),
            value,
            matching_content_path: None,
            config_path: None,
        }
    }

    #[tokio::test]
    async fn success_returns_confirmation() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(
            "Updated threshold 'function_lines_of_code_warning' to value 120 in ruleset matching_content_path='**/*'.",
        ));
        let result = server
            .rules_config_set_threshold(Parameters(params("function_lines_of_code_warning", 120)))
            .await
            .unwrap();
        assert_success_contains(&result, "Updated threshold");
    }

    #[tokio::test]
    async fn unknown_threshold_surfaces_cli_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(
            1,
            "Unknown threshold 'made_up_name'",
        ));
        let result = server
            .rules_config_set_threshold(Parameters(params("made_up_name", 10)))
            .await
            .unwrap();
        assert_error_contains(&result, "Unknown threshold");
    }

    #[tokio::test]
    async fn flag_like_threshold_name_is_rejected() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok("unused"));
        let result = server
            .rules_config_set_threshold(Parameters(params("--evil", 10)))
            .await
            .unwrap();
        assert_error_contains(&result, "must not start with '-'");
    }
}
