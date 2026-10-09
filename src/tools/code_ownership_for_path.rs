use std::path::Path;

use rmcp::model::{CallToolResult, ContentBlock as Content};
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::analytics_attribution::AnalyticsContext;
use crate::api_client;
use crate::auth::AuthCredential;
use crate::docker;
use crate::event_properties;
use crate::tools::common::{latest_analysis_id, make_relative_for_api, tool_error};
use crate::tools::OwnershipParam;
use crate::{CodeSceneServer, ContextualErrorEvent};

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: OwnershipParam,
) -> Result<CallToolResult, ErrorData> {
    let analytics_context = AnalyticsContext::ExplicitProjectIds(vec![params.project_id]);
    let credential = match server
        .require_project_api("code-ownership", params.project_id)
        .await
    {
        Ok(credential) => credential,
        Err(result) => return Ok(result),
    };
    server.version_checker.check_in_background();
    let path = docker::adapt_path_for_docker(Path::new(&params.path));
    let relative = make_relative_for_api(Path::new(&path));
    let analysis_id =
        match latest_analysis_id(server, &credential, params.project_id, "code-ownership").await {
            Ok(id) => id,
            Err(result) => return Ok(result),
        };
    let analysis_endpoint = format!("v2/projects/{}/analyses/{analysis_id}", params.project_id);
    let endpoint = format!("{analysis_endpoint}/files");
    let query_params = vec![
        ("filter".to_string(), format!("path~{}", relative)),
        ("fields".to_string(), "owner,path".to_string()),
    ];
    let result = api_client::query_api_keyed_list_with_auth(
        &endpoint,
        &query_params,
        "files",
        &*server.http_client,
        Some(&credential),
    )
    .await;
    match result {
        Ok(mut data) => {
            enrich_ownership(server, &credential, &analysis_endpoint, &mut data).await;
            let props =
                event_properties::ownership_properties(params.project_id, Path::new(&params.path));
            server.track_with_context("code-ownership", props, analytics_context);
            let text = serde_json::to_string(&data).unwrap_or_default();
            let text = server.maybe_version_warning(&text).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_contextual_err(
                ContextualErrorEvent::for_project(e.kind(), "code-ownership", params.project_id),
                &e,
            );
            Ok(tool_error(format!("Error: {e}")))
        }
    }
}

async fn enrich_ownership(
    server: &CodeSceneServer,
    credential: &AuthCredential,
    analysis_endpoint: &str,
    files: &mut [Value],
) {
    if files.is_empty() {
        return;
    }
    let endpoint = format!("{analysis_endpoint}/author-statistics");
    let statistics =
        api_client::query_api_with_auth(&endpoint, &*server.http_client, Some(credential)).await;
    let statuses = statistics
        .ok()
        .and_then(author_statuses)
        .unwrap_or_default();
    for file in files {
        annotate_owner(file, &statuses);
    }
}

#[derive(Deserialize)]
struct AuthorStatus {
    author: String,
    former_contributor: Option<bool>,
}

fn author_statuses(data: Value) -> Option<HashMap<String, Option<bool>>> {
    let authors: Vec<AuthorStatus> = serde_json::from_value(data).ok()?;
    let mut statuses = HashMap::new();
    for author in authors {
        statuses
            .entry(author.author)
            .and_modify(|status| {
                if *status != author.former_contributor {
                    *status = None;
                }
            })
            .or_insert(author.former_contributor);
    }
    Some(statuses)
}

fn annotate_owner(file: &mut Value, statuses: &HashMap<String, Option<bool>>) {
    let former = file
        .get("owner")
        .and_then(Value::as_str)
        .and_then(|owner| statuses.get(owner))
        .copied()
        .flatten();
    let (status, note) = match former {
        Some(true) => ("former_contributor", "Historical owner; do not recommend as a current reviewer. Confirm a handover or current owner."),
        Some(false) => ("current_contributor", "Not marked as a former contributor; confirm availability before assigning review."),
        None => ("unknown", "Contributor status could not be verified; do not recommend a current reviewer from this ownership record."),
    };
    if let Some(object) = file.as_object_mut() {
        object.insert("former_contributor".into(), serde_json::json!(former));
        object.insert("owner_status".into(), serde_json::json!(status));
        object.insert(
            "reviewer_candidate".into(),
            serde_json::json!(former == Some(false)),
        );
        object.insert("ownership_note".into(), serde_json::json!(note));
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

    async fn run_ownership(server: crate::CodeSceneServer) -> rmcp::model::CallToolResult {
        server
            .code_ownership_for_path(Parameters(OwnershipParam {
                project_id: 1,
                path: "/tmp/f.rs".into(),
            }))
            .await
            .unwrap()
    }

    fn make_api_mock(response: HttpResponse) -> MockHttpClient {
        MockHttpClient::new(vec![
            HttpResponse::ok(r#"{"id":42}"#),
            response,
            HttpResponse::ok(r#"[{"author":"Alice","former_contributor":false}]"#),
        ])
    }

    #[tokio::test]
    async fn rejects_standalone_mode() {
        let _g = set_token("test-token");
        assert_standalone_error(&run_ownership(make_server(true)).await);
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        assert_token_error(&run_ownership(make_server(false)).await);
    }

    #[tokio::test]
    async fn success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(
            r#"{"files":[{"owner":"Alice","path":"src/f.rs"}],"page":1,"max_pages":1}"#,
        ));
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let params = OwnershipParam {
            project_id: 5,
            path: "/tmp/src/f.rs".to_string(),
        };
        let result = server
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        let data: serde_json::Value =
            serde_json::from_str(crate::tests::result_text(&result)).unwrap();
        assert_eq!(data[0]["owner"], "Alice");
        assert_eq!(data[0]["former_contributor"], false);
        assert_eq!(data[0]["reviewer_candidate"], true);
    }

    async fn ownership_response(files: &str, statistics: HttpResponse) -> serde_json::Value {
        let http = MockHttpClient::new(vec![
            HttpResponse::ok(r#"{"id":42}"#),
            HttpResponse::ok(files),
            statistics,
        ]);
        let requests = http.captured_requests.clone();
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let result = server
            .code_ownership_for_path(Parameters(OwnershipParam {
                project_id: 5,
                path: "src".into(),
            }))
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));
        let data: serde_json::Value =
            serde_json::from_str(crate::tests::result_text(&result)).unwrap();
        let requests = requests.lock().unwrap();
        let expected_requests = if data.as_array().unwrap().is_empty() {
            2
        } else {
            3
        };
        assert_eq!(requests.len(), expected_requests);
        assert!(requests[1].url.contains("/analyses/42/files"));
        if requests.len() == 3 {
            assert!(requests[2].url.ends_with("/analyses/42/author-statistics"));
        }
        data
    }

    #[tokio::test]
    async fn former_and_current_owners_are_distinguished() {
        let _g = set_token("tok");
        let data = ownership_response(
            r#"{"files":[{"owner":"Former","path":"src/old.rs"},{"owner":"Current","path":"src/new.rs"}],"page":1,"max_pages":1}"#,
            HttpResponse::ok(r#"[{"author":"Former","former_contributor":true},{"author":"Current","former_contributor":false}]"#),
        ).await;
        assert_eq!(data[0]["owner"], "Former");
        assert_eq!(data[0]["path"], "src/old.rs");
        assert_eq!(data[0]["former_contributor"], true);
        assert_eq!(data[0]["owner_status"], "former_contributor");
        assert_eq!(data[0]["reviewer_candidate"], false);
        assert_eq!(data[1]["owner_status"], "current_contributor");
        assert_eq!(data[1]["reviewer_candidate"], true);
    }

    #[tokio::test]
    async fn unavailable_statistics_preserve_owner_with_unknown_status() {
        let _g = set_token("tok");
        let data = ownership_response(
            r#"{"files":[{"owner":"Alice","path":"src/f.rs"}],"page":1,"max_pages":1}"#,
            HttpResponse::error(404, "Not supported"),
        )
        .await;
        assert_eq!(data[0]["owner"], "Alice");
        assert_eq!(data[0]["owner_status"], "unknown");
        assert!(data[0]["former_contributor"].is_null());
        assert_eq!(data[0]["reviewer_candidate"], false);
    }

    #[tokio::test]
    async fn empty_ownership_skips_author_lookup() {
        let _g = set_token("tok");
        let data = ownership_response(
            r#"{"files":[],"page":1,"max_pages":1}"#,
            HttpResponse::error(500, "Must not be requested"),
        )
        .await;
        assert_eq!(data, serde_json::json!([]));
    }

    #[test]
    fn missing_and_conflicting_statuses_are_unknown() {
        let statuses = super::author_statuses(serde_json::json!([
            {"author":"Missing"},
            {"author":"Conflict","former_contributor":true},
            {"author":"Conflict","former_contributor":false},
        ]))
        .unwrap();
        for owner in ["Missing", "Conflict", "Unlisted"] {
            let mut file = serde_json::json!({"owner":owner,"path":"src/f.rs"});
            super::annotate_owner(&mut file, &statuses);
            assert_eq!(file["owner_status"], "unknown");
            assert_eq!(file["reviewer_candidate"], false);
        }
    }

    #[test]
    fn invalid_statistics_are_not_treated_as_current_contributors() {
        for statistics in [
            serde_json::json!({}),
            serde_json::json!([
                {"author":"Alice","former_contributor":"false"}
            ]),
        ] {
            assert!(super::author_statuses(statistics).is_none());
        }
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
