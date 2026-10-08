use std::path::Path;

use rmcp::model::{CallToolResult, ContentBlock as Content};
use rmcp::ErrorData;

use crate::analytics_attribution::AnalyticsContext;
use crate::business_case::{self, BusinessCase, HealthScore, TargetError};
use crate::docker;
use crate::event_properties;
use crate::tools::common::{extract_score, run_review, tool_error};
use crate::tools::validation::CliCheck;
use crate::tools::BusinessCaseParam;
use crate::{CodeSceneServer, ContextualErrorEvent};

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: BusinessCaseParam,
) -> Result<CallToolResult, ErrorData> {
    let target = params.target_code_health.map(HealthScore);
    if let Some(error) = out_of_range_target_error(target) {
        return Ok(error);
    }
    let analytics_context = AnalyticsContext::Path(params.file_path.clone().into());
    if let Some(r) = server
        .require_token_with_context(
            "code-health-refactoring-business-case",
            analytics_context.clone(),
        )
        .await
    {
        return Ok(r);
    }
    server.version_checker.check_in_background();
    let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
    let fp = Path::new(&file_path);
    if let Err(e) = server.validator.run_checks(&[
        CliCheck::FileExists(fp),
        CliCheck::SupportedFileType(fp),
        CliCheck::InsideGitRepo(fp),
    ]) {
        server.track_contextual_err(
            ContextualErrorEvent {
                error_kind: e.kind,
                tool: "code-health-refactoring-business-case",
                detail: e.detail.as_deref(),
                context: analytics_context.clone(),
            },
            &e,
        );
        return Ok(tool_error(&e.message));
    }
    let content_hash = event_properties::hash_file_content(fp);
    let review_result = run_review(fp, &*server.cli_runner).await;
    match review_result {
        Ok(output) => {
            let result_text = business_case_text(&output, target);
            let props = event_properties::business_case_properties(
                Path::new(&params.file_path),
                content_hash.as_deref(),
                &result_text,
            );
            server.track_with_context(
                "code-health-refactoring-business-case",
                props,
                analytics_context,
            );
            let text = server.maybe_version_warning(&result_text).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_contextual_err(
                ContextualErrorEvent {
                    error_kind: e.kind(),
                    tool: "code-health-refactoring-business-case",
                    detail: None,
                    context: analytics_context,
                },
                &e,
            );
            Ok(tool_error(format!("Error: {e}")))
        }
    }
}

fn out_of_range_target_error(target: Option<HealthScore>) -> Option<CallToolResult> {
    let target = target.filter(|t| !t.is_valid_target())?;
    Some(tool_error(out_of_range_message(target)))
}

fn out_of_range_message(target: HealthScore) -> String {
    format!(
        "Invalid target_code_health {}: must be between {} and {}.",
        target.value(),
        HealthScore::MIN.value(),
        HealthScore::MAX.value()
    )
}

fn business_case_text(review_output: &str, target: Option<HealthScore>) -> String {
    let Some(score) = extract_score(review_output).map(HealthScore) else {
        return "Could not determine Code Health score.".into();
    };
    match target {
        Some(target) => targeted_case_text(score, target),
        None => incremental_case_text(score),
    }
}

fn incremental_case_text(score: HealthScore) -> String {
    match business_case::make_business_case(score.value()) {
        Some(case) => to_json(&case),
        None => "Code Health is already optimal. No business case needed.".into(),
    }
}

fn targeted_case_text(score: HealthScore, target: HealthScore) -> String {
    match business_case::make_business_case_for_target(score, target) {
        Ok(case) => to_json(&case),
        Err(TargetError::NotAboveCurrent) => format!(
            "The file's current Code Health ({}) already meets the target \
             ({}). Choose a higher target_code_health, or omit it to get \
             the next incremental target.",
            score.value(),
            target.value()
        ),
        Err(TargetError::OutOfRange) => out_of_range_message(target),
    }
}

fn to_json(case: &BusinessCase) -> String {
    serde_json::to_string_pretty(case).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::model::CallToolResult;

    use crate::tests::{
        assert_error_contains, assert_success_contains, assert_token_error, clear_token,
        make_cli_mock_server, make_failing_validator_server, make_server, set_token, MockCliRunner,
    };
    use crate::tools::BusinessCaseParam;
    use crate::CodeSceneServer;

    async fn run_business_case_with_target(
        server: &CodeSceneServer,
        target_code_health: Option<f64>,
    ) -> CallToolResult {
        let params = BusinessCaseParam {
            file_path: "/tmp/test.rs".to_string(),
            target_code_health,
        };
        server
            .code_health_refactoring_business_case(Parameters(params))
            .await
            .unwrap()
    }

    async fn run_business_case(server: &CodeSceneServer) -> CallToolResult {
        run_business_case_with_target(server, None).await
    }

    fn score_server(score: f64) -> CodeSceneServer {
        make_cli_mock_server(MockCliRunner::with_ok(&format!(
            r#"{{"score":{score},"review":[]}}"#
        )))
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = BusinessCaseParam {
            file_path: "/tmp/f.rs".to_string(),
            target_code_health: None,
        };
        let result = make_server(false)
            .code_health_refactoring_business_case(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn validation_failure_returns_error() {
        let _g = set_token("tok");
        let server = make_failing_validator_server("not_a_git_repo", "Not inside a git repository");
        let result = run_business_case(&server).await;
        assert_error_contains(&result, "Not inside a git repository");
    }

    #[tokio::test]
    async fn success_with_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":6.0,"review":[]}"#));
        let result = run_business_case(&server).await;
        assert_success_contains(&result, "scenario");
    }

    #[tokio::test]
    async fn optimal_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":10.0,"review":[]}"#));
        let result = run_business_case(&server).await;
        assert_success_contains(&result, "already optimal");
    }

    #[tokio::test]
    async fn no_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"review":[]}"#));
        let result = run_business_case(&server).await;
        assert_success_contains(&result, "Could not determine");
    }

    #[tokio::test]
    async fn cli_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "review failed"));
        let result = run_business_case(&server).await;
        assert_error_contains(&result, "review failed");
    }

    #[tokio::test]
    async fn default_targets_next_incremental_scenario() {
        let _g = set_token("tok");
        let result = run_business_case(&score_server(8.0)).await;
        assert_success_contains(&result, "\"target_score\": 9.1");
    }

    #[tokio::test]
    async fn user_selected_target_overrides_incremental_scenario() {
        let _g = set_token("tok");
        let result = run_business_case_with_target(&score_server(8.0), Some(10.0)).await;
        assert_success_contains(&result, "\"target_score\": 10.0");
        assert_success_contains(&result, "\"scenario\": \"optimal\"");
    }

    #[tokio::test]
    async fn custom_target_is_labelled_user_selected() {
        let _g = set_token("tok");
        let result = run_business_case_with_target(&score_server(8.0), Some(9.5)).await;
        assert_success_contains(&result, "user-selected target");
    }

    #[tokio::test]
    async fn target_not_above_current_explains_why() {
        let _g = set_token("tok");
        let result = run_business_case_with_target(&score_server(9.5), Some(9.1)).await;
        assert_success_contains(&result, "already meets the target");
    }

    #[tokio::test]
    async fn out_of_range_target_is_rejected() {
        let _g = set_token("tok");
        let result = run_business_case_with_target(&score_server(8.0), Some(11.0)).await;
        assert_error_contains(&result, "must be between 1 and 10");
    }
}
