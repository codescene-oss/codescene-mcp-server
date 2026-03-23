use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::api_client;
use crate::docker;
use crate::event_properties;
use crate::tools::common::{make_relative_for_api, tool_error, urlencoded};
use crate::tools::OwnershipParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: OwnershipParam,
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
    let path = docker::adapt_path_for_docker(Path::new(&params.path));
    let relative = make_relative_for_api(&path);
    let endpoint = format!(
        "v2/projects/{}/ownership?path={}",
        params.project_id,
        urlencoded(&relative)
    );
    let result = api_client::query_api_list_with_client(&endpoint, &*server.http_client).await;
    match result {
        Ok(data) => {
            let props =
                event_properties::ownership_properties(params.project_id, Path::new(&params.path));
            server.track("code-ownership", props);
            let text = serde_json::to_string(&data).unwrap_or_default();
            let text = server.maybe_version_warning(&text).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("code-ownership", &e.to_string());
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
    use crate::tools::OwnershipParam;

    fn make_api_mock(response: HttpResponse) -> MockHttpClient {
        MockHttpClient::new(vec![response, HttpResponse::ok("[]")])
    }

    #[tokio::test]
    async fn rejects_standalone_mode() {
        let _g = set_token("test-token");
        let params = OwnershipParam {
            project_id: 1,
            path: "/tmp/f.rs".to_string(),
        };
        let result = make_server(true)
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = OwnershipParam {
            project_id: 1,
            path: "/tmp/f.rs".to_string(),
        };
        let result = make_server(false)
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"owner":"Alice","paths":["src/"]}]"#));
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let params = OwnershipParam {
            project_id: 5,
            path: "/tmp/src/f.rs".to_string(),
        };
        let result = server
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        assert!(crate::tests::result_text(&result).contains("Alice"));
    }

    #[tokio::test]
    async fn api_error() {
        let _g = set_token("tok");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![HttpResponse::error(401, "Unauthorized")]),
        );
        let params = OwnershipParam {
            project_id: 5,
            path: "/tmp/src/f.rs".to_string(),
        };
        let result = server
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }
}
