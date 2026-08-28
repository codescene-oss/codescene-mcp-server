use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::{json, Value};

use crate::api_client;
use crate::tools::common::tool_error;
use crate::CodeSceneServer;

const ENDPOINT: &str = "v2/code-health/safeguards/mcp/insights/me";
const OUTCOMES_ENDPOINT: &str =
    "v2/code-health/safeguards/mcp/outcomes/me?page=1&page-size=100";

pub(crate) async fn handle(server: &CodeSceneServer) -> Result<CallToolResult, ErrorData> {
    let credential = match server.resolve_auth_credential().await {
        Ok(credential) => credential,
        Err(result) => return Ok(result),
    };
    if server.is_standalone {
        return Ok(tool_error(
            "This tool requires a CodeScene API token (not a standalone license).",
        ));
    }
    server.version_checker.check_in_background();

    let insights = api_client::query_api_with_auth(
        ENDPOINT,
        &*server.http_client,
        Some(&credential),
    )
    .await;
    let insights = match insights {
        Ok(insights) => insights,
        Err(error) => return Ok(api_error_result(server, "insights", error)),
    };
    let outcomes = api_client::query_api_with_auth(
        OUTCOMES_ENDPOINT,
        &*server.http_client,
        Some(&credential),
    )
    .await;
    match outcomes {
        Ok(mut outcomes) => {
            remove_user_identities(&mut outcomes);
            server.track("show-mcp-usage-overview", json!({}));
            Ok(usage_result(insights, outcomes))
        }
        Err(error) => Ok(api_error_result(server, "outcomes", error)),
    }
}

fn api_error_result(
    server: &CodeSceneServer,
    source: &str,
    error: crate::errors::ApiError,
) -> CallToolResult {
    server.track_api_err("show-mcp-usage-overview", &error);
    tool_error(&format!("Error fetching MCP usage {source}: {error}"))
}

fn remove_user_identities(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("user_identity");
            object.values_mut().for_each(remove_user_identities);
        }
        Value::Array(items) => items.iter_mut().for_each(remove_user_identities),
        _ => {}
    }
}

fn usage_result(insights: Value, outcomes: Value) -> CallToolResult {
    let payload = json!({ "insights": insights, "recent_outcomes": outcomes });
    let text = markdown_fallback(&payload);
    let mut result = CallToolResult::success(vec![Content::text(text)]);
    result.structured_content = Some(payload);
    result
}

fn markdown_fallback(payload: &Value) -> String {
    let summary = &payload["insights"]["summary"];
    let outcomes = payload["recent_outcomes"]["outcomes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let recent = recent_summary(outcomes);
    let tools = markdown_count_table(&summary["most_used_tools"], "Tool");
    let versions = markdown_version_table(&summary["versions"]);
    let findings = markdown_pairs(&recent.categories, "Finding");
    let environments = markdown_pairs(&recent.environments, "Environment");
    format!(
        "# MCP safeguard usage\n\n\
         ## Lifetime overview\n\n\
         | Metric | Value |\n| --- | ---: |\n\
         | Safeguard checks | {} |\n\
         | Declines prevented | {} |\n\
         | Code Health uplifts | {} |\n\
         | Active installations | {} |\n\n\
         ## Most-used tools\n\n{tools}\n\n\
         ## Active versions\n\n{versions}\n\n\
         ## Recent activity (latest 100 events)\n\n\
         | Metric | Value |\n| --- | ---: |\n\
         | Files reviewed | {} |\n\
         | Average Code Health | {} |\n\
         | Perfect scores | {} |\n\
         | Gate pass rate | {} |\n\n\
         ### Common findings\n\n{findings}\n\n\
         ### Environments\n\n{environments}",
        number(summary, "number_of_safeguard_checks"),
        number(summary, "number_of_degradations_prevented"),
        number(summary, "number_of_uplifts"),
        number(summary, "number_of_active_installations"),
        recent.files,
        formatted_average(recent.score_total, recent.scores),
        formatted_percent(recent.perfect_scores, recent.scores),
        formatted_percent(recent.passed_gates, recent.gates),
    )
}

#[derive(Default)]
struct RecentSummary {
    files: usize,
    score_total: f64,
    scores: usize,
    perfect_scores: usize,
    gates: usize,
    passed_gates: usize,
    categories: std::collections::BTreeMap<String, usize>,
    environments: std::collections::BTreeMap<String, usize>,
}

fn recent_summary(outcomes: &[Value]) -> RecentSummary {
    let mut summary = RecentSummary::default();
    let mut files = std::collections::HashSet::new();
    for outcome in outcomes {
        collect_score(outcome, &mut summary);
        collect_properties(outcome, &mut summary, &mut files);
        for category in string_array(&outcome["categories"]) {
            *summary.categories.entry(category.to_string()).or_default() += 1;
        }
    }
    summary.files = files.len();
    summary
}

fn collect_score(outcome: &Value, summary: &mut RecentSummary) {
    if let Some(score) = outcome["score"].as_f64() {
        summary.score_total += score;
        summary.scores += 1;
        summary.perfect_scores += usize::from(score == 10.0);
    }
}

fn collect_properties(
    outcome: &Value,
    summary: &mut RecentSummary,
    files: &mut std::collections::HashSet<String>,
) {
    let properties = &outcome["event_properties"];
    if let Some(file) = properties["file_hash"].as_str() {
        files.insert(file.to_string());
    }
    if let Some(environment) = properties["environment"].as_str() {
        *summary.environments.entry(environment.to_string()).or_default() += 1;
    }
    if let Some(gate) = properties["quality_gates"].as_str() {
        summary.gates += 1;
        summary.passed_gates += usize::from(gate == "passed");
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or_default()
}

fn string_array(value: &Value) -> impl Iterator<Item = &str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn formatted_average(total: f64, count: usize) -> String {
    (count > 0)
        .then(|| format!("{:.2}", total / count as f64))
        .unwrap_or_else(|| "-".to_string())
}

fn formatted_percent(part: usize, total: usize) -> String {
    (total > 0)
        .then(|| format!("{:.0}%", part as f64 / total as f64 * 100.0))
        .unwrap_or_else(|| "-".to_string())
}

fn markdown_count_table(value: &Value, heading: &str) -> String {
    let rows = value
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| format!("| {} | {} |", item["tool"].as_str().unwrap_or("-"), number(item, "count")))
        .collect::<Vec<_>>()
        .join("\n");
    format!("| {heading} | Runs |\n| --- | ---: |\n{rows}")
}

fn markdown_version_table(value: &Value) -> String {
    let rows = value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| number(item, "number_of_active_installs") > 0)
        .map(|item| format!("| {} | {} |", item["version"].as_str().unwrap_or("-"), number(item, "number_of_active_installs")))
        .collect::<Vec<_>>()
        .join("\n");
    format!("| Version | Active installs |\n| --- | ---: |\n{rows}")
}

fn markdown_pairs(values: &std::collections::BTreeMap<String, usize>, heading: &str) -> String {
    let mut sorted: Vec<_> = values.iter().collect();
    sorted.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    let rows = sorted
        .into_iter()
        .take(5)
        .map(|(name, count)| format!("| {name} | {count} |"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("| {heading} | Count |\n| --- | ---: |\n{rows}")
}

#[cfg(test)]
mod tests {
    use crate::http::{tests::MockHttpClient, HttpResponse};
    use crate::tests::{
        assert_standalone_error, assert_token_error, clear_token, make_server,
        make_server_with_mocks, set_token, MockCliRunner,
    };

    const RESPONSE: &str = r#"{
        "meta": {"generated_at":"2026-08-28T07:31:56Z","user_identity":50203},
        "summary": {
            "most_used_tools": [{"tool":"code-health-review","count":31}],
            "number_of_active_installations": 12,
            "number_of_degradations_prevented": 3,
            "number_of_installations": 18,
            "number_of_safeguard_checks": 42,
            "number_of_uplifts": 7,
            "versions": [{"version":"1.4.7","number_of_active_installs":6}]
        }
    }"#;
    const OUTCOMES: &str = r#"{
        "page":1,
        "max_pages":2,
        "outcomes":[{
            "event_type":"code-health-review",
            "timestamp":"2026-08-28T08:13:56Z",
            "event_properties":{"file_hash":"abc","environment":"binary"},
            "score":10.0,
            "user_identity":"private@example.com"
        }]
    }"#;
    const DETAILED_OUTCOMES: &str = r#"{
        "page":1,
        "max_pages":1,
        "outcomes":[
            {
                "score":10.0,
                "categories":["Complex Method","Complex Method"],
                "event_properties":{
                    "file_hash":"abc",
                    "environment":"binary",
                    "quality_gates":"passed"
                }
            },
            {
                "score":8.0,
                "event_properties":{
                    "file_hash":"def",
                    "environment":"cs-agent",
                    "quality_gates":"failed"
                }
            }
        ]
    }"#;

    #[tokio::test]
    async fn returns_api_insights_as_structured_content() {
        let _guard = set_token("tok");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![HttpResponse::ok(RESPONSE), HttpResponse::ok(OUTCOMES)]),
        );

        let result = server.show_mcp_usage_overview().await.unwrap();

        let content = result.structured_content.as_ref().unwrap();
        assert_eq!(
            content["insights"]["summary"]
                ["number_of_safeguard_checks"],
            42
        );
        assert!(content["recent_outcomes"]["outcomes"][0]
            .get("user_identity")
            .is_none());
        let text = crate::tests::result_text(&result);
        assert!(text.contains("| Safeguard checks | 42 |"));
        assert!(text.contains("| Files reviewed | 1 |"));
        assert!(!text.contains("private@example.com"));
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _guard = clear_token();
        let result = make_server(false).show_mcp_usage_overview().await.unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn rejects_standalone_mode() {
        let _guard = set_token("tok");
        let result = make_server(true).show_mcp_usage_overview().await.unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn reports_api_errors() {
        let _guard = set_token("tok");
        for responses in [
            vec![HttpResponse::error(500, "API down")],
            vec![
                HttpResponse::ok(RESPONSE),
                HttpResponse::error(500, "API down"),
            ],
        ] {
            let server = make_server_with_mocks(
                false,
                MockCliRunner::with_responses(vec![]),
                MockHttpClient::new(responses),
            );
            let result = server.show_mcp_usage_overview().await.unwrap();
            assert_eq!(result.is_error, Some(true));
        }
    }

    #[tokio::test]
    async fn markdown_summarizes_recent_findings_and_gates() {
        let _guard = set_token("tok");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![
                HttpResponse::ok(RESPONSE),
                HttpResponse::ok(DETAILED_OUTCOMES),
            ]),
        );

        let result = server.show_mcp_usage_overview().await.unwrap();
        let text = crate::tests::result_text(&result);

        assert!(text.contains("| Files reviewed | 2 |"));
        assert!(text.contains("| Average Code Health | 9.00 |"));
        assert!(text.contains("| Gate pass rate | 50% |"));
        assert!(text.contains("| Complex Method | 2 |"));
    }

    #[test]
    fn tool_metadata_links_to_ui_resource() {
        let tool = crate::CodeSceneServer::show_mcp_usage_overview_tool_attr();
        let serialized = serde_json::to_value(tool).unwrap();

        assert_eq!(
            serialized["_meta"]["ui"]["resourceUri"],
            crate::server_handler::MCP_USAGE_APP_URI
        );
    }
}
