//! Analytics environment override integration tests.
//!
//! Validates that:
//! - Default environment (binary/docker) is sent when `CS_ENVIRONMENT` is unset
//! - Custom environment value is sent when `CS_ENVIRONMENT` is set
//! Uses get_config and local endpoints so no CLI license or live API is needed.

use super::fake_http_server::FakeHttpServer;
use super::*;

const TIMEOUT: Duration = Duration::from_secs(60);
const TOOL_NAME: &str = "get_config";
const EVENT_TYPE: &str = "mcp-get-config";

fn config_event_environment(extra_env: &[(&str, &str)]) -> String {
    let server = FakeHttpServer::always_ok();
    let (command, mut env, repo_dir, _tmp) = setup();

    use_isolated_config_dir(&mut env, &repo_dir, ".cs_config_analytics_env");
    for (key, value) in [
        ("CS_ENVIRONMENT", ""),
        ("CS_DISABLE_TRACKING", "0"),
        ("CS_DISABLE_VERSION_CHECK", "1"),
        ("CS_ACCESS_TOKEN", "analytics-environment-test-token"),
        ("CS_ONPREM_URL", &server.url()),
        ("CS_TRACKING_URL", &server.url()),
    ]
    .into_iter()
    .chain(extra_env.iter().copied())
    {
        env.retain(|(name, _)| name != key);
        env.push((key.to_string(), value.to_string()));
    }

    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(TOOL_NAME, json!({}), TIMEOUT)
        .expect("Tool call should succeed");

    assert!(!extract_result_text(&response).is_empty(), "{response}");

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let payloads = server.get_payloads();
        let environment = payloads
            .iter()
            .find(|p| p.get("event-type").and_then(|v| v.as_str()) == Some(EVENT_TYPE))
            .and_then(|p| p.get("event-properties"))
            .and_then(|p| p.get("environment"))
            .and_then(|v| v.as_str())
            .map(String::from);
        if let Some(environment) = environment {
            return environment;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Timed out waiting for {EVENT_TYPE} environment; captured payloads: {payloads:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn test_default_environment_is_sent() {
    let value = config_event_environment(&[]);

    assert!(
        value == "binary" || value == "docker",
        "Default environment should be 'binary' or 'docker', got: '{value}'"
    );
}

pub fn test_overridden_environment_is_sent() {
    let override_value = "my-agent-name";
    let value = config_event_environment(&[("CS_ENVIRONMENT", override_value)]);

    assert_eq!(
        value, override_value,
        "Environment should be '{override_value}', got: '{value}'"
    );
}
