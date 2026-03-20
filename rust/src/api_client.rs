/// CodeScene API client — mirrors Python's `codescene_api_client.py`.
///
/// Handles paginated API requests to the CodeScene platform.

use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde_json::Value;
use tokio::time::Duration;

use crate::errors::ApiError;

/// Resolve the CodeScene API base URL.
pub fn get_api_url() -> String {
    if let Ok(url) = std::env::var("CS_ONPREM_URL") {
        format!("{url}/api")
    } else {
        "https://api.codescene.io".to_string()
    }
}

/// Make an authenticated GET request to the CodeScene API.
pub async fn query_api(endpoint: &str) -> Result<Value, ApiError> {
    let url = format!("{}/{}", get_api_url(), endpoint.trim_start_matches('/'));
    let token = std::env::var("CS_ACCESS_TOKEN").unwrap_or_default();

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(ACCEPT, "application/json")
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Status {
            status: status.as_u16(),
            body,
        });
    }

    Ok(resp.json().await?)
}

/// Make a paginated API query, collecting all pages into a single vec.
pub async fn query_api_list(endpoint: &str) -> Result<Vec<Value>, ApiError> {
    let mut results = Vec::new();
    let mut page = 1;

    loop {
        let sep = if endpoint.contains('?') { '&' } else { '?' };
        let paged = format!("{endpoint}{sep}page={page}");
        let data = query_api(&paged).await?;

        let items = match data.as_array() {
            Some(arr) => arr,
            None => return Ok(collect_with_single(results, data)),
        };
        if items.is_empty() {
            break;
        }
        results.extend(items.iter().cloned());
        page += 1;
    }

    Ok(results)
}

fn collect_with_single(mut results: Vec<Value>, item: Value) -> Vec<Value> {
    results.push(item);
    results
}
