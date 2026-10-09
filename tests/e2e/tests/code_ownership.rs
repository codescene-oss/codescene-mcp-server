//! Code ownership integration tests through the MCP stdio transport.
//!
//! A local HTTPS API supplies historical owners and contributor status so
//! reviewer eligibility is tested without depending on live project data.
//! Captured requests verify pagination and a consistent analysis snapshot.

use super::fake_https_server::{CapturedRequest, FakeHttpsServer};
use super::*;
use serde_json::Value;

const TOOL_NAME: &str = "code_ownership_for_path";
const TIMEOUT: Duration = Duration::from_secs(60);
const ANALYSIS_PATH: &str = "/api/v2/projects/42/analyses/123";

fn file_records(owners: &[&str]) -> Vec<Value> {
    owners
        .iter()
        .map(|owner| {
            json!({
                "owner": owner,
                "path": format!("src/{owner}.rs"),
            })
        })
        .collect()
}

fn api_response(
    request: &CapturedRequest,
    files: &[Value],
    statistics: &(u16, Value),
) -> (u16, String) {
    let url = reqwest::Url::parse(&format!("https://localhost{}", request.path)).unwrap();
    match url.path() {
        "/api/v2/projects/42/analyses/latest" => (200, json!({"id":123}).to_string()),
        "/api/v2/projects/42/analyses/123/files" => {
            let page: usize = url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .expect("page parameter")
                .1
                .parse()
                .expect("numeric page");
            let items: Vec<_> = files.iter().skip(page - 1).take(1).collect();
            (
                200,
                json!({"files":items,"page":page,"max_pages":files.len().max(1)}).to_string(),
            )
        }
        "/api/v2/projects/42/analyses/123/author-statistics" => {
            (statistics.0, statistics.1.to_string())
        }
        _ => (404, json!({"error":"Unexpected endpoint"}).to_string()),
    }
}

fn ownership_env(
    mut env: Vec<(String, String)>,
    server: &FakeHttpsServer,
    repo_dir: &Path,
) -> Vec<(String, String)> {
    let overrides = [
        ("CS_ACCESS_TOKEN", "ownership-e2e-token".to_string()),
        ("CS_ONPREM_URL", server.url()),
        (
            "REQUESTS_CA_BUNDLE",
            docker_ca_bundle(&server.certs.ca_cert_path, repo_dir),
        ),
        ("CS_DISABLE_VERSION_CHECK", "1".to_string()),
        ("CS_DISABLE_TRACKING", "1".to_string()),
        ("CS_DISABLE_SETUP_HINT", "1".to_string()),
        ("CS_ENABLED_TOOLS", TOOL_NAME.to_string()),
    ];
    env.retain(|(key, _)| !overrides.iter().any(|(name, _)| key == name));
    env.extend(
        overrides
            .into_iter()
            .map(|(key, value)| (key.to_string(), value)),
    );
    use_isolated_config_dir(&mut env, repo_dir, "ownership-config");
    env
}

fn assert_requests(server: &FakeHttpsServer, file_count: usize) {
    let requests = server.get_requests();
    let analysis_requests: Vec<_> = requests
        .iter()
        .filter(|request| request.path.contains("/analyses/"))
        .collect();
    let expected_count = 1 + file_count.max(1) + usize::from(file_count > 0);
    assert_eq!(
        analysis_requests.len(),
        expected_count,
        "Unexpected analysis requests"
    );
    assert_eq!(
        analysis_requests[0].path,
        "/api/v2/projects/42/analyses/latest"
    );
    for request in analysis_requests.iter().skip(1) {
        assert_eq!(request.method, "GET");
        assert!(
            request.path.starts_with(ANALYSIS_PATH),
            "Requests must use the resolved analysis snapshot"
        );
        if request.path.contains("/files?") {
            let url = reqwest::Url::parse(&format!("https://localhost{}", request.path)).unwrap();
            let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
            assert_eq!(
                query.get("filter").map(|value| value.as_ref()),
                Some("path~src")
            );
            assert_eq!(
                query.get("fields").map(|value| value.as_ref()),
                Some("owner,path")
            );
        }
    }
}

fn run_case(owners: &[&str], statistics: (u16, Value)) -> Vec<Value> {
    let (command, env, repo_dir, _tmp) = setup();
    let files = file_records(owners);
    let served_files = files.clone();
    let server = FakeHttpsServer::start(&repo_dir, move |request| {
        api_response(request, &served_files, &statistics)
    });
    let env = ownership_env(env, &server, &repo_dir);
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    let response = client
        .call_tool(TOOL_NAME, json!({"project_id":42,"path":"src"}), TIMEOUT)
        .expect("Ownership tool should respond");
    assert_eq!(
        response["result"]["isError"], false,
        "Ownership should succeed: {response}"
    );
    let records: Vec<Value> =
        serde_json::from_str(&extract_result_text(&response)).expect("Ownership array");
    assert_eq!(
        records.len(),
        files.len(),
        "All paginated owners should be retained"
    );
    for (record, original) in records.iter().zip(&files) {
        assert_eq!(record["owner"], original["owner"]);
        assert_eq!(record["path"], original["path"]);
    }
    assert_requests(&server, owners.len());
    server.shutdown();
    records
}

fn assert_status(record: &Value, former: Option<bool>) {
    let expected_status = match former {
        Some(true) => "former_contributor",
        Some(false) => "current_contributor",
        None => "unknown",
    };
    assert_eq!(record["former_contributor"], json!(former));
    assert_eq!(record["owner_status"], expected_status);
    assert_eq!(record["reviewer_candidate"], former == Some(false));
    assert!(!record["ownership_note"]
        .as_str()
        .expect("Routing explanation")
        .is_empty());
}

pub fn test_ownership_current_and_former_contributors() {
    let records = run_case(
        &["Current", "Former"],
        (
            200,
            json!([
                {"author":"Current","former_contributor":false},
                {"author":"Former","former_contributor":true},
            ]),
        ),
    );
    assert_status(&records[0], Some(false));
    assert_status(&records[1], Some(true));
}

pub fn test_ownership_missing_and_conflicting_status() {
    let records = run_case(
        &["Missing", "Null", "Unlisted", "Conflict"],
        (
            200,
            json!([
                {"author":"Missing"},
                {"author":"Null","former_contributor":null},
                {"author":"Conflict","former_contributor":true},
                {"author":"Conflict","former_contributor":false},
            ]),
        ),
    );
    for record in records {
        assert_status(&record, None);
    }
}

pub fn test_ownership_unavailable_author_statistics() {
    for status in [404, 503] {
        let records = run_case(
            &["Historical"],
            (status, json!({"error":"Statistics unavailable"})),
        );
        assert_status(&records[0], None);
    }
}

pub fn test_ownership_malformed_author_statistics() {
    for statistics in [
        json!({}),
        json!([{"author":"Historical","former_contributor":"false"}]),
    ] {
        let records = run_case(&["Historical"], (200, statistics));
        assert_status(&records[0], None);
    }
}

pub fn test_ownership_empty_results_skip_statistics() {
    assert!(run_case(&[], (500, json!({"error":"Must not be requested"}))).is_empty());
}
