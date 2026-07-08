//! Response parsing utilities for MCP tool responses.

use regex::Regex;
use serde_json::Value;

/// Extract the actual result text from an MCP response.
pub fn extract_result_text(response: &Value) -> String {
    if let Some(text) = extract_from_content(response) {
        return text;
    }
    extract_from_structured_content(response).unwrap_or_default()
}

fn extract_from_content(response: &Value) -> Option<String> {
    let first = response
        .get("result")?
        .get("content")?
        .as_array()?
        .first()?;
    first.get("text")?.as_str().map(String::from)
}

fn extract_from_structured_content(response: &Value) -> Option<String> {
    response
        .get("result")?
        .get("structuredContent")?
        .get("result")?
        .as_str()
        .map(String::from)
}

/// Extract Code Health score from response text.
pub fn extract_code_health_score(response_text: &str) -> Option<f64> {
    let text_lower = response_text.to_lowercase();
    let patterns = [
        r"code health score[:\s]+([0-9]+\.?[0-9]*)",
        r"score[:\s]+([0-9]+\.?[0-9]*)",
        r"health[:\s]+([0-9]+\.?[0-9]*)",
    ];

    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .find_map(|re| parse_first_capture(&re, &text_lower))
}

fn parse_first_capture(re: &Regex, text: &str) -> Option<f64> {
    re.captures(text)?.get(1)?.as_str().parse().ok()
}
