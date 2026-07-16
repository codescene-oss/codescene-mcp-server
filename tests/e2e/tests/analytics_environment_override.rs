//! Analytics environment override integration tests.
//!
//! Validates that:
//! - Default environment (binary/docker) is sent when `CS_ENVIRONMENT` is unset
//! - Custom environment value is sent when `CS_ENVIRONMENT` is set

use super::fake_http_server::FakeHttpServer;
use super::*;

const TIMEOUT: Duration = Duration::from_secs(60);

fn score_event_environment(extra_env: &[(&str, &str)]) -> Option<String> {
    let server = FakeHttpServer::always_ok();
    let (command, mut env, repo_dir, _tmp) = setup();

    use_isolated_config_dir(&mut env, &repo_dir, ".cs_config_analytics_env");
    env.retain(|(k, _)| k != "CS_DISABLE_TRACKING");
    env.push(("CS_TRACKING_URL".to_string(), server.url()));
    for (key, val) in extra_env {
        env.push((key.to_string(), val.to_string()));
    }

    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/utils/calculator.py");
    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let score = extract_code_health_score(&result_text);
    assert!(score.is_some(), "Should get a valid score: {result_text}");

    std::thread::sleep(Duration::from_secs(2));

    let payloads = server.get_payloads();
    let props = payloads
        .iter()
        .find(|p| p.get("event-type").and_then(|v| v.as_str()) == Some("mcp-code-health-score"))
        .and_then(|p| p.get("event-properties"));

    props
        .and_then(|p| p.get("environment"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub fn test_default_environment_is_sent() {
    let value = score_event_environment(&[]).expect("Should find environment property");

    assert!(
        value == "binary" || value == "docker",
        "Default environment should be 'binary' or 'docker', got: '{value}'"
    );
}

pub fn test_overridden_environment_is_sent() {
    let override_value = "my-agent-name";
    let value = score_event_environment(&[("CS_ENVIRONMENT", override_value)])
        .expect("Should find environment property");

    assert_eq!(
        value, override_value,
        "Environment should be '{override_value}', got: '{value}'"
    );
}
