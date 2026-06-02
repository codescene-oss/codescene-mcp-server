//! Integration tests for HTTP headers sent by the API client.
//!
//! Validates that outgoing requests include the expected User-Agent,
//! Accept, and Authorization headers.

use super::*;
use super::fake_http_server::FakeHttpServer;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let lower = name.to_lowercase();
    headers
        .iter()
        .find(|(k, _)| k.to_lowercase() == lower)
        .map(|(_, v)| v.as_str())
}

pub fn test_api_client_headers() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();
    let server = FakeHttpServer::start(move |_req| {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            (200, r#"[{"id":1,"name":"Test Project"}]"#.to_string())
        } else {
            (200, "[]".to_string())
        }
    });

    let temp_dir = create_temp_dir("cs_mcp_headers_").expect("create temp dir");
    let sample_files = get_sample_files();
    let repo_dir =
        create_git_repo(temp_dir.path(), &sample_files).expect("create git repo");

    let executable = find_or_build_executable();
    let backend = create_backend(executable);
    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let command = backend.get_command(&repo_dir);

    let env: Vec<(String, String)> = env_map
        .into_iter()
        .chain([
            ("CS_ONPREM_URL".to_string(), server.url()),
            ("CS_ACCESS_TOKEN".to_string(), "test-token-for-header-check".to_string()),
            ("CS_DISABLE_VERSION_CHECK".to_string(), "1".to_string()),
            ("CS_DISABLE_TRACKING".to_string(), "1".to_string()),
        ])
        .collect();

    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let _response = client
        .call_tool("select_project", json!({}), Duration::from_secs(30))
        .expect("select_project should succeed");

    server.shutdown();

    let requests = server.get_requests();
    assert!(
        !requests.is_empty(),
        "Server should have received at least one request"
    );

    for req in &requests {
        let user_agent = find_header(&req.headers, "User-Agent")
            .unwrap_or_else(|| panic!("Missing User-Agent header on {}", req.path));
        assert!(
            user_agent.starts_with("codescene-mcp/"),
            "User-Agent should start with 'codescene-mcp/', got: {user_agent}"
        );

        let accept = find_header(&req.headers, "Accept")
            .unwrap_or_else(|| panic!("Missing Accept header on {}", req.path));
        assert!(
            accept.contains("application/json"),
            "Accept should contain 'application/json', got: {accept}"
        );

        let auth = find_header(&req.headers, "Authorization")
            .unwrap_or_else(|| panic!("Missing Authorization header on {}", req.path));
        assert!(
            auth.starts_with("Bearer "),
            "Authorization should start with 'Bearer ', got: {auth}"
        );
        assert!(
            auth.len() > "Bearer ".len(),
            "Authorization should have content after 'Bearer '"
        );
    }
}
