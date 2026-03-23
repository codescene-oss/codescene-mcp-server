use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::api_client;
use crate::event_properties;
use crate::tools::common::tool_error;
use crate::tools::ProjectParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: ProjectParam,
) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token() {
        return Ok(r);
    }
    if server.is_standalone {
        return Ok(tool_error(
            "This tool requires a CodeScene API token (not a standalone license).",
        ));
    }
    server.version_checker.check_in_background();
    let endpoint = format!("v2/projects/{}/hotspots", params.project_id);
    let result = api_client::query_api_list_with_client(&endpoint, &*server.http_client).await;
    match result {
        Ok(data) => {
            let props = event_properties::hotspots_properties(params.project_id, data.len());
            server.track("list-technical-debt-hotspots", props);
            let text = serde_json::to_string(&data).unwrap_or_default();
            let text = server.maybe_version_warning(&text).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("list-technical-debt-hotspots", &e.to_string());
            Ok(tool_error(&format!("Error: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::http::{tests::MockHttpClient, HttpResponse};
    use crate::tests::{
        assert_standalone_error, assert_token_error, clear_token, make_server,
        make_server_with_mocks, set_token, MockCliRunner,
    };
    use crate::tools::ProjectParam;

    fn make_api_mock(response: HttpResponse) -> MockHttpClient {
        MockHttpClient::new(vec![response, HttpResponse::ok("[]")])
    }

    #[tokio::test]
    async fn rejects_standalone_mode() {
        let _g = set_token("test-token");
        let result = make_server(true)
            .list_technical_debt_hotspots_for_project(Parameters(ProjectParam { project_id: 1 }))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let result = make_server(false)
            .list_technical_debt_hotspots_for_project(Parameters(ProjectParam { project_id: 1 }))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn project_success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"file":"b.rs","score":3.5}]"#));
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let params = ProjectParam { project_id: 10 };
        let result = server
            .list_technical_debt_hotspots_for_project(Parameters(params))
            .await
            .unwrap();
        assert!(crate::tests::result_text(&result).contains("b.rs"));
    }

    #[tokio::test]
    async fn project_api_error() {
        let _g = set_token("tok");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![HttpResponse::error(403, "Forbidden")]),
        );
        let params = ProjectParam { project_id: 10 };
        let result = server
            .list_technical_debt_hotspots_for_project(Parameters(params))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }
}
