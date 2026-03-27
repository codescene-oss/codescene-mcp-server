use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::json;

use crate::api_client;
use crate::event_properties;
use crate::tools::common::tool_error;
use crate::CodeSceneServer;

pub(crate) async fn handle(server: &CodeSceneServer) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token() {
        return Ok(r);
    }
    if server.is_standalone {
        return Ok(tool_error(
            "This tool requires a CodeScene API token (not a standalone license).",
        ));
    }
    server.version_checker.check_in_background();
    let result = run_select_project(&*server.http_client).await;
    match &result {
        Ok(output) => {
            let props = event_properties::select_project_properties();
            server.track("select-project", props);
            let text = serde_json::to_string(output).unwrap_or_default();
            let text = server.maybe_version_warning(&text).await;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                text,
            )]))
        }
        Err(e) => {
            server.track_err("select-project", e);
            Ok(tool_error(e))
        }
    }
}

pub(crate) async fn run_select_project(
    http_client: &dyn crate::http::HttpClient,
) -> Result<serde_json::Value, String> {
    let link = std::env::var("CS_ONPREM_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://codescene.io/projects".to_string());

    if let Ok(id_str) = std::env::var("CS_DEFAULT_PROJECT_ID") {
        if !id_str.is_empty() {
            let id: i64 = id_str.parse().unwrap_or(0);
            return Ok(json!({
                "id": id,
                "name": "Default Project (from CS_DEFAULT_PROJECT_ID env var)",
                "description": "Using default project from CS_DEFAULT_PROJECT_ID environment variable. If you want to be able to select a different project, unset this variable.",
                "link": link,
            }));
        }
    }

    let data = api_client::query_api_list_with_client("v2/projects", http_client)
        .await
        .map_err(|e| format!("Error: {e}"))?;

    Ok(json!({ "projects": data, "link": link }))
}

#[cfg(test)]
mod tests {
    use super::run_select_project;

    use rmcp::handler::server::wrapper::Parameters;

    use crate::http::{tests::MockHttpClient, HttpResponse};
    use crate::tests::{
        assert_standalone_error, assert_success_contains, clear_token, make_server,
        make_server_with_mocks, result_text, set_token, MockCliRunner,
    };
    use crate::tools::ProjectParam;

    fn make_api_mock(response: HttpResponse) -> MockHttpClient {
        MockHttpClient::new(vec![response, HttpResponse::ok("[]")])
    }

    #[tokio::test]
    async fn rejects_standalone_mode() {
        let _g = set_token("test-token");
        let result = make_server(true).select_project().await.unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let result = make_server(false).select_project().await.unwrap();
        assert!(result_text(&result).contains("No access token configured"));
    }

    #[tokio::test]
    async fn run_with_default_project_id() {
        let _g = set_token("test-token");
        std::env::set_var("CS_DEFAULT_PROJECT_ID", "42");
        let result = run_select_project(&crate::http::ReqwestClient).await;
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let value = result.unwrap();
        assert_eq!(value["id"], 42);
        assert!(value["description"].as_str().unwrap().contains("default"));
    }

    #[tokio::test]
    async fn run_default_id_empty_falls_through() {
        let _g = set_token("test-token");
        std::env::set_var("CS_DEFAULT_PROJECT_ID", "");
        let result = run_select_project(&crate::http::ReqwestClient).await;
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        match result {
            Ok(val) => assert!(val.get("projects").is_some()),
            Err(e) => assert!(e.contains("Error")),
        }
    }

    #[tokio::test]
    async fn api_success() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = make_api_mock(HttpResponse::ok(r#"[{"id":1,"name":"My Project"}]"#));
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let result = server.select_project().await.unwrap();
        assert_success_contains(&result, "My Project");
    }

    #[tokio::test]
    async fn api_error() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = MockHttpClient::new(vec![HttpResponse::error(500, "API down")]);
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let result = server.select_project().await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn run_api_call_success() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = make_api_mock(HttpResponse::ok(r#"[{"id":99,"name":"TestProject"}]"#));
        let result = run_select_project(&http).await.unwrap();
        assert!(result["projects"].as_array().unwrap().len() > 0);
        assert_eq!(result["projects"][0]["name"], "TestProject");
    }

    #[tokio::test]
    async fn run_api_call_error() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = MockHttpClient::new(vec![HttpResponse::error(500, "fail")]);
        let result = run_select_project(&http).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Error"));
    }

    #[allow(dead_code)]
    fn _dummy(_p: ProjectParam, _x: Parameters<ProjectParam>) {}
}
