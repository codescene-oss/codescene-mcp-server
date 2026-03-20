/// Analytics event property extractors — mirrors Python's `event_properties.py`.
///
/// Each tool has a specific extractor that derives non-PII properties from
/// tool arguments and results. Uses SHA-256 truncated hashes for file paths.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::hashing::truncated_sha256;

/// Identifies a config tool invocation as either a read or write operation.
#[derive(Debug, Clone, Copy)]
pub enum ConfigAction {
    Get,
    Set,
}

impl ConfigAction {
    fn as_str(self) -> &'static str {
        match self {
            ConfigAction::Get => "get",
            ConfigAction::Set => "set",
        }
    }
}

/// Extract properties for a code health review event.
pub fn review_properties(file_path: &Path, result: &str) -> Value {
    let mut props = json!({ "file-hash": hash_path(file_path) });
    if let Some(data) = parse_json_dict(result) {
        if let Some(score) = data.get("score") {
            props["score"] = score.clone();
        }
        if let Some(review) = data.get("review").and_then(|r| r.as_array()) {
            let refs: Vec<&Value> = review.iter().collect();
            add_categories(&mut props, &categories_from_entries(&refs));
        }
    }
    props
}

/// Extract properties for a code health score event.
pub fn score_properties(file_path: &Path, score: Option<f64>) -> Value {
    json!({
        "file-hash": hash_path(file_path),
        "score": score,
    })
}

/// Extract properties for a pre-commit safeguard event.
pub fn pre_commit_properties(repo_path: &Path, result: &str) -> Value {
    let mut props = json!({ "repo-hash": hash_path(repo_path) });
    merge_delta_properties(&mut props, result);
    props
}

/// Extract properties for an analyze change set event.
pub fn change_set_properties(repo_path: &Path, base_ref: &Path, result: &str) -> Value {
    let mut props = json!({
        "repo-hash": hash_path(repo_path),
        "base-ref-hash": hash_path(base_ref),
    });
    merge_delta_properties(&mut props, result);
    props
}

/// Extract properties for an auto-refactor event.
pub fn refactor_properties(file_path: &Path, result: &Value) -> Value {
    let mut props = json!({ "file-hash": hash_path(file_path) });
    if let Some(confidence) = result.get("confidence").and_then(|c| c.as_str()) {
        props["confidence"] = json!(confidence);
    }
    props
}

/// Extract properties for a business case event.
pub fn business_case_properties(file_path: &Path, result: &str) -> Value {
    let mut props = json!({ "file-hash": hash_path(file_path) });
    if let Some(data) = parse_json_dict(result) {
        if let Some(outcome) = data.get("outcome").and_then(|o| o.as_object()) {
            if let Some(v) = outcome.get("current_code_health") {
                props["current-code-health"] = v.clone();
            }
            if let Some(v) = outcome.get("target_code_health") {
                props["target-code-health"] = v.clone();
            }
        }
    }
    props
}

/// Extract properties for a select project event (empty per Python).
pub fn select_project_properties() -> Value {
    json!({})
}

/// Extract properties for a technical debt goals event (empty per Python).
pub fn goals_properties(_project_id: i64, _goal_count: usize) -> Value {
    json!({})
}

/// Extract properties for a technical debt goals for file event.
pub fn goals_file_properties(file_path: &Path) -> Value {
    json!({ "file-hash": hash_path(file_path) })
}

/// Extract properties for a technical debt hotspots event (empty per Python).
pub fn hotspots_properties(_project_id: i64, _hotspot_count: usize) -> Value {
    json!({})
}

/// Extract properties for a technical debt hotspots for file event.
pub fn hotspots_file_properties(file_path: &Path) -> Value {
    json!({ "file-hash": hash_path(file_path) })
}

/// Extract properties for a code ownership event.
pub fn ownership_properties(_project_id: i64, path: &Path) -> Value {
    json!({ "path-hash": hash_path(path) })
}

/// Extract properties for a config event (get or set).
pub fn config_properties(action: ConfigAction, key: &str) -> Value {
    json!({
        "action": action.as_str(),
        "key": key,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Hash a filesystem path for non-PII analytics.
fn hash_path(path: &Path) -> String {
    truncated_sha256(&path.to_string_lossy())
}

/// Parse a string as JSON and return it only if it's a dict/object.
fn parse_json_dict(s: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(s).ok()?;
    if v.is_object() { Some(v) } else { None }
}

/// Extract shared delta analysis properties from the tool result string.
///
/// The result is a JSON string `{"quality_gates": "...", "results": [...]}`.
fn merge_delta_properties(props: &mut Value, result: &str) {
    let data = match parse_json_dict(result) {
        Some(d) => d,
        None => return,
    };

    if let Some(qg) = data.get("quality_gates").and_then(|v| v.as_str()) {
        props["quality-gates"] = json!(qg);
    }

    let results = match data.get("results").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return,
    };

    props["file-count"] = json!(results.len());

    let verdicts = count_verdicts(results);
    if !verdicts.is_empty() {
        props["verdicts"] = json!(verdicts);
    }

    let findings: Vec<&Value> = results
        .iter()
        .filter_map(|e| e.get("findings"))
        .filter_map(|f| f.as_array())
        .flatten()
        .collect();
    add_categories(props, &categories_from_entries(&findings));
}

/// Count verdict occurrences across delta result entries.
fn count_verdicts(results: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in results {
        if let Some(v) = entry.get("verdict").and_then(|v| v.as_str()) {
            *counts.entry(v.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

/// Return sorted unique category strings from a list of finding entries.
fn categories_from_entries(entries: &[&Value]) -> Vec<String> {
    let mut cats: Vec<String> = entries
        .iter()
        .filter_map(|e| e.get("category").and_then(|c| c.as_str()))
        .map(|s| s.to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    cats.sort();
    cats
}

/// Add categories and category-count to props if non-empty.
fn add_categories(props: &mut Value, categories: &[String]) {
    if !categories.is_empty() {
        props["categories"] = json!(categories);
        props["category-count"] = json!(categories.len());
    }
}
