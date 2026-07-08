//! `rules_config_set_rule` — enable or disable a Code Health rule.

use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::event_properties;
use crate::tools::common::tool_error;
use crate::tools::rules_config::{Invocation, Subcommand};
use crate::tools::RulesConfigSetRuleParam;
use crate::CodeSceneServer;

const TOOL: &str = "rules-config-set-rule";

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: RulesConfigSetRuleParam,
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
                "set-rule",
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

fn build_invocation(params: &RulesConfigSetRuleParam) -> Result<Invocation, String> {
    Invocation::new(Subcommand::SetRule)
        .option("--rule-name", &params.rule_name)
        .map(|inv| inv.trusted_option("--enabled", params.enabled.to_string()))
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
    use crate::tools::RulesConfigSetRuleParam;

    fn params(rule_name: &str, enabled: bool) -> RulesConfigSetRuleParam {
        RulesConfigSetRuleParam {
            rule_name: rule_name.to_string(),
            enabled,
            matching_content_path: None,
            config_path: None,
        }
    }

    #[tokio::test]
    async fn success_returns_confirmation() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(
            "Updated rule 'Complex Method' to disabled in ruleset matching_content_path='**/*'.",
        ));
        let result = server
            .rules_config_set_rule(Parameters(params("Complex Method", false)))
            .await
            .unwrap();
        assert_success_contains(&result, "Updated rule");
    }

    #[tokio::test]
    async fn unknown_rule_surfaces_cli_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(
            1,
            "Unknown code health rule 'Nope'",
        ));
        let result = server
            .rules_config_set_rule(Parameters(params("Nope", false)))
            .await
            .unwrap();
        assert_error_contains(&result, "Unknown code health rule");
    }

    #[tokio::test]
    async fn flag_like_rule_name_is_rejected() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok("unused"));
        let result = server
            .rules_config_set_rule(Parameters(params("--evil", true)))
            .await
            .unwrap();
        assert_error_contains(&result, "must not start with '-'");
    }

    #[tokio::test]
    async fn multiple_rule_sets_error_is_surfaced() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(
            1,
            "Multiple rule sets detected. Provide --matching-content-path to select the ruleset to edit.",
        ));
        let result = server
            .rules_config_set_rule(Parameters(params("Complex Method", false)))
            .await
            .unwrap();
        assert_error_contains(&result, "Multiple rule sets detected");
    }
}
