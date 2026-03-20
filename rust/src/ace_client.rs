/// ACE refactoring API client — mirrors Python's `ace_api_client.py`.
///
/// POSTs to the ACE API for auto-refactoring with retry on 408/504.

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use tokio::time::Duration;

use crate::errors::ApiError;

/// Default ACE API endpoint.
const ACE_API_URL: &str = "https://devtools.codescene.io/api/refactor";

/// Maximum number of retry attempts on transient errors.
const MAX_RETRIES: u32 = 3;

/// Status codes that trigger a retry.
const RETRYABLE_CODES: &[u16] = &[408, 504];

/// Send a refactoring request to the ACE API.
pub async fn refactor(payload: &Value) -> Result<Value, ApiError> {
    let url = ace_url();
    let token = std::env::var("CS_ACE_ACCESS_TOKEN").unwrap_or_default();

    let client = reqwest::Client::new();
    let mut last_error = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
        }

        let resp = client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(payload)
            .timeout(Duration::from_secs(120))
            .send()
            .await?;

        let status = resp.status().as_u16();

        if resp.status().is_success() {
            return Ok(resp.json().await?);
        }

        let body = resp.text().await.unwrap_or_default();

        if RETRYABLE_CODES.contains(&status) {
            last_error = Some(ApiError::Status { status, body });
            continue;
        }

        return Err(ApiError::Status { status, body });
    }

    Err(last_error.unwrap_or(ApiError::Status {
        status: 500,
        body: "Max retries exceeded".to_string(),
    }))
}

fn ace_url() -> String {
    std::env::var("CS_ACE_API_URL").unwrap_or_else(|_| ACE_API_URL.to_string())
}
