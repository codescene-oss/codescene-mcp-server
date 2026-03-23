use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::event_properties;
use crate::tools::common::{run_auto_refactor, tool_error};
use crate::tools::RefactorParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: RefactorParam,
) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token() {
        return Ok(r);
    }
    server.version_checker.check_in_background();
    let result = run_auto_refactor(
        Path::new(&params.file_path),
        &params.function_name,
        &*server.cli_runner,
        &*server.http_client,
    )
    .await;
    match &result {
        Ok(result_json) => {
            let props =
                event_properties::refactor_properties(Path::new(&params.file_path), result_json);
            server.track("code-health-auto-refactor", props);
            let text = server
                .maybe_version_warning(&serde_json::to_string(result_json).unwrap_or_default())
                .await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("code-health-auto-refactor", e);
            let text = server.maybe_version_warning(e).await;
            Ok(tool_error(&text))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rmcp::handler::server::wrapper::Parameters;

    use crate::http::{tests::MockHttpClient, HttpResponse};
    use crate::tests::{
        assert_error_contains, assert_success_contains, assert_token_error, clear_token,
        make_server, make_server_with_mocks, set_token, MockCliRunner,
    };
    use crate::tools::common::run_auto_refactor;
    use crate::tools::RefactorParam;

    fn make_refactor_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        std::env::set_var("CS_ACE_ACCESS_TOKEN", "ace-tok");
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let file_path = dir.path().join("test.js");
        std::fs::write(&file_path, "function foo() {}").unwrap();
        (dir, file_path)
    }

    async fn refactor_with_fixture(
        cli: MockCliRunner,
        http: MockHttpClient,
        fn_name: &str,
    ) -> Result<serde_json::Value, String> {
        let (_dir, file_path) = make_refactor_fixture();
        let result = run_auto_refactor(&file_path, fn_name, &cli, &http).await;
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        result
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = RefactorParam {
            file_path: "/tmp/f.rs".to_string(),
            function_name: "foo".to_string(),
        };
        let result = make_server(false)
            .code_health_auto_refactor(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn auto_refactor_missing_ace_token() {
        let _g = set_token("tok");
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![]),
        );
        let params = RefactorParam {
            file_path: "/tmp/test.js".to_string(),
            function_name: "foo".to_string(),
        };
        let result = server
            .code_health_auto_refactor(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "ACE access");
    }

    #[tokio::test]
    async fn run_auto_refactor_no_git_root() {
        let _g = set_token("tok");
        std::env::set_var("CS_ACE_ACCESS_TOKEN", "ace-tok");
        let result = run_auto_refactor(
            Path::new("/nonexistent/path/test.js"),
            "foo",
            &MockCliRunner::with_responses(vec![]),
            &MockHttpClient::new(vec![]),
        )
        .await;
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        assert!(result.unwrap_err().contains("git root"));
    }

    #[tokio::test]
    async fn run_auto_refactor_parse_fns_error() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "parse-fns failed");
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("parse-fns failed"));
    }

    #[tokio::test]
    async fn run_auto_refactor_invalid_parse_fns_json() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_ok("not-json");
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("parsing functions"));
    }

    #[tokio::test]
    async fn run_auto_refactor_review_error() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Err(crate::errors::CliError::NonZeroExit {
                code: 1,
                stderr: "review exploded".to_string(),
            }),
        ]);
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("review exploded"));
    }

    #[tokio::test]
    async fn run_auto_refactor_function_not_found() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"bar","body":"function bar(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[]}"#.to_string()),
        ]);
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("Couldn't find function: foo"));
    }

    #[tokio::test]
    async fn run_auto_refactor_no_code_smells() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":10.0,"review":[]}"#.to_string()),
        ]);
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("No code smells"));
    }

    #[tokio::test]
    async fn run_auto_refactor_ace_api_error() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[{"category":"Complex Method","functions":[{"title":"foo","start-line":1}]}]}"#.to_string()),
        ]);
        let http = MockHttpClient::new(vec![HttpResponse::error(500, "ACE down")]);
        let result = refactor_with_fixture(cli, http, "foo").await;
        assert!(result.unwrap_err().contains("ACE down"));
    }

    #[tokio::test]
    async fn run_auto_refactor_full_success() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo() { complex(); }","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[{"category":"Complex Method","functions":[{"title":"foo","start-line":1}]}]}"#.to_string()),
        ]);
        let ace_response = r#"{"code":"function foo() { simple(); }","confidence":{"description":"high"},"reasons":[{"summary":"Simplified"}]}"#;
        let http = MockHttpClient::new(vec![HttpResponse::ok(ace_response)]);
        let result = refactor_with_fixture(cli, http, "foo").await;
        let value = result.unwrap();
        assert_eq!(value["confidence"], "high");
        assert!(value["code"].as_str().unwrap().contains("simple"));
    }

    #[tokio::test]
    async fn auto_refactor_tool_success() {
        let _g = set_token("tok");
        let (_dir, file_path) = make_refactor_fixture();
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo() { x(); }","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[{"category":"Complex Method","functions":[{"title":"foo","start-line":1}]}]}"#.to_string()),
        ]);
        let ace_resp =
            r#"{"code":"function foo() {}","confidence":{"description":"high"},"reasons":[]}"#;
        let http = MockHttpClient::new(vec![HttpResponse::ok(ace_resp)]);
        let server = make_server_with_mocks(false, cli, http);
        let params = RefactorParam {
            file_path: file_path.to_string_lossy().to_string(),
            function_name: "foo".to_string(),
        };
        let result = server
            .code_health_auto_refactor(Parameters(params))
            .await
            .unwrap();
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        assert_success_contains(&result, "high");
    }

    #[tokio::test]
    async fn auto_refactor_tool_error_path() {
        let _g = set_token("tok");
        std::env::set_var("CS_ACE_ACCESS_TOKEN", "ace-tok");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![]),
        );
        let params = RefactorParam {
            file_path: "/nonexistent/test.js".to_string(),
            function_name: "foo".to_string(),
        };
        let result = server
            .code_health_auto_refactor(Parameters(params))
            .await
            .unwrap();
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        assert_error_contains(&result, "git root");
    }
}
