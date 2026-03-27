use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::hashing::truncated_sha256;

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

pub fn score_properties(file_path: &Path, score: Option<f64>) -> Value {
    json!({
        "file-hash": hash_path(file_path),
        "score": score,
    })
}

pub fn pre_commit_properties(repo_path: &Path, result: &str) -> Value {
    let mut props = json!({ "repo-hash": hash_path(repo_path) });
    merge_delta_properties(&mut props, result);
    props
}

pub fn change_set_properties(repo_path: &Path, base_ref: &Path, result: &str) -> Value {
    let mut props = json!({
        "repo-hash": hash_path(repo_path),
        "base-ref-hash": hash_path(base_ref),
    });
    merge_delta_properties(&mut props, result);
    props
}

pub fn refactor_properties(file_path: &Path, result: &Value) -> Value {
    let mut props = json!({ "file-hash": hash_path(file_path) });
    if let Some(confidence) = result.get("confidence").and_then(|c| c.as_str()) {
        props["confidence"] = json!(confidence);
    }
    props
}

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

pub fn select_project_properties() -> Value {
    json!({})
}

pub fn goals_properties(_project_id: i64, _goal_count: usize) -> Value {
    json!({})
}

pub fn goals_file_properties(file_path: &Path) -> Value {
    json!({ "file-hash": hash_path(file_path) })
}

pub fn hotspots_properties(_project_id: i64, _hotspot_count: usize) -> Value {
    json!({})
}

pub fn hotspots_file_properties(file_path: &Path) -> Value {
    json!({ "file-hash": hash_path(file_path) })
}

pub fn ownership_properties(_project_id: i64, path: &Path) -> Value {
    json!({ "path-hash": hash_path(path) })
}

pub fn config_properties(action: ConfigAction, key: &str) -> Value {
    json!({
        "action": action.as_str(),
        "key": key,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn hash_path(path: &Path) -> String {
    truncated_sha256(&path.to_string_lossy())
}

fn parse_json_dict(s: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(s).ok()?;
    if v.is_object() {
        Some(v)
    } else {
        None
    }
}

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

fn count_verdicts(results: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in results {
        if let Some(v) = entry.get("verdict").and_then(|v| v.as_str()) {
            *counts.entry(v.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

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

fn add_categories(props: &mut Value, categories: &[String]) {
    if !categories.is_empty() {
        props["categories"] = json!(categories);
        props["category-count"] = json!(categories.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ---- ConfigAction ----

    #[test]
    fn config_action_as_str() {
        assert_eq!(ConfigAction::Get.as_str(), "get");
        assert_eq!(ConfigAction::Set.as_str(), "set");
    }

    // ---- hash_path ----

    #[test]
    fn hash_path_is_deterministic() {
        let a = hash_path(Path::new("/foo/bar"));
        let b = hash_path(Path::new("/foo/bar"));
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn hash_path_differs_for_different_paths() {
        let a = hash_path(Path::new("/foo/bar"));
        let b = hash_path(Path::new("/baz/qux"));
        assert_ne!(a, b);
    }

    // ---- parse_json_dict ----

    #[test]
    fn parse_json_dict_returns_none_for_number() {
        assert!(parse_json_dict("42").is_none());
    }

    #[test]
    fn parse_json_dict_returns_none_for_array() {
        assert!(parse_json_dict("[1,2]").is_none());
    }

    #[test]
    fn parse_json_dict_returns_none_for_string() {
        assert!(parse_json_dict("\"hello\"").is_none());
    }

    #[test]
    fn parse_json_dict_returns_none_for_invalid() {
        assert!(parse_json_dict("invalid").is_none());
    }

    #[test]
    fn parse_json_dict_returns_some_for_object() {
        let result = parse_json_dict(r#"{"key":"value"}"#);
        assert!(result.is_some());
    }

    // ---- score_properties ----

    #[test]
    fn score_properties_with_score() {
        let props = score_properties(Path::new("/test.rs"), Some(8.5));
        assert!(props.get("file-hash").is_some());
        assert_eq!(props["score"], json!(8.5));
    }

    #[test]
    fn score_properties_without_score() {
        let props = score_properties(Path::new("/test.rs"), None);
        assert!(props["score"].is_null());
    }

    // ---- review_properties ----

    #[test]
    fn review_properties_includes_file_hash_and_score() {
        let result = r#"{"score": 7.5, "review": [{"category": "Complex Method"}]}"#;
        let props = review_properties(Path::new("/test.rs"), result);
        assert!(props.get("file-hash").is_some());
        assert_eq!(props["score"], json!(7.5));
    }

    #[test]
    fn review_properties_includes_categories() {
        let result = r#"{"score": 7.5, "review": [{"category": "Complex Method"}]}"#;
        let props = review_properties(Path::new("/test.rs"), result);
        assert_eq!(props["categories"], json!(["Complex Method"]));
        assert_eq!(props["category-count"], json!(1));
    }

    #[test]
    fn review_properties_with_invalid_json() {
        let props = review_properties(Path::new("/test.rs"), "not json");
        assert!(props.get("file-hash").is_some());
        assert!(props.get("score").is_none());
    }

    #[test]
    fn review_properties_no_review_key() {
        let result = r#"{"score": 10.0}"#;
        let props = review_properties(Path::new("/test.rs"), result);
        assert_eq!(props["score"], json!(10.0));
        assert!(props.get("categories").is_none());
    }

    // ---- pre_commit_properties ----

    #[test]
    fn pre_commit_properties_includes_repo_hash() {
        let result = r#"{"quality_gates":"passed","results":[{"verdict":"stable","findings":[{"category":"Large Method"}]}]}"#;
        let props = pre_commit_properties(Path::new("/repo"), result);
        assert!(props.get("repo-hash").is_some());
        assert_eq!(props["quality-gates"], json!("passed"));
    }

    #[test]
    fn pre_commit_properties_includes_verdict_data() {
        let result = r#"{"quality_gates":"passed","results":[{"verdict":"stable","findings":[{"category":"Large Method"}]}]}"#;
        let props = pre_commit_properties(Path::new("/repo"), result);
        assert_eq!(props["file-count"], json!(1));
        assert_eq!(props["verdicts"], json!({"stable": 1}));
    }

    #[test]
    fn pre_commit_properties_includes_categories() {
        let result = r#"{"quality_gates":"passed","results":[{"verdict":"stable","findings":[{"category":"Large Method"}]}]}"#;
        let props = pre_commit_properties(Path::new("/repo"), result);
        assert_eq!(props["categories"], json!(["Large Method"]));
    }

    #[test]
    fn pre_commit_properties_with_invalid_result() {
        let props = pre_commit_properties(Path::new("/repo"), "not json");
        assert!(props.get("repo-hash").is_some());
        assert!(props.get("quality-gates").is_none());
    }

    // ---- change_set_properties ----

    #[test]
    fn change_set_properties_includes_hashes() {
        let result = r#"{"quality_gates":"failed","results":[]}"#;
        let props = change_set_properties(Path::new("/repo"), Path::new("main"), result);
        assert!(props.get("repo-hash").is_some());
        assert!(props.get("base-ref-hash").is_some());
    }

    #[test]
    fn change_set_properties_includes_gates_and_count() {
        let result = r#"{"quality_gates":"failed","results":[]}"#;
        let props = change_set_properties(Path::new("/repo"), Path::new("main"), result);
        assert_eq!(props["quality-gates"], json!("failed"));
        assert_eq!(props["file-count"], json!(0));
    }

    // ---- refactor_properties ----

    #[test]
    fn refactor_properties_with_confidence() {
        let result = json!({"confidence": "high", "code": "fn x() {}"});
        let props = refactor_properties(Path::new("/test.rs"), &result);
        assert!(props.get("file-hash").is_some());
        assert_eq!(props["confidence"], json!("high"));
    }

    #[test]
    fn refactor_properties_without_confidence() {
        let result = json!({"code": "fn x() {}"});
        let props = refactor_properties(Path::new("/test.rs"), &result);
        assert!(props.get("file-hash").is_some());
        assert!(props.get("confidence").is_none());
    }

    // ---- business_case_properties ----

    #[test]
    fn business_case_properties_with_outcome() {
        let result = r#"{"outcome":{"current_code_health":3.0,"target_code_health":10.0}}"#;
        let props = business_case_properties(Path::new("/test.rs"), result);
        assert!(props.get("file-hash").is_some());
        assert_eq!(props["current-code-health"], json!(3.0));
        assert_eq!(props["target-code-health"], json!(10.0));
    }

    #[test]
    fn business_case_properties_without_outcome() {
        let props = business_case_properties(Path::new("/test.rs"), "{}");
        assert!(props.get("file-hash").is_some());
        assert!(props.get("current-code-health").is_none());
    }

    // ---- simple property functions ----

    #[test]
    fn select_project_properties_is_empty() {
        let props = select_project_properties();
        assert!(props.as_object().unwrap().is_empty());
    }

    #[test]
    fn goals_properties_is_empty() {
        let props = goals_properties(42, 5);
        assert!(props.as_object().unwrap().is_empty());
    }

    #[test]
    fn goals_file_properties_has_hash() {
        let props = goals_file_properties(Path::new("/src/foo.rs"));
        assert!(props.get("file-hash").is_some());
    }

    #[test]
    fn hotspots_properties_is_empty() {
        let props = hotspots_properties(42, 10);
        assert!(props.as_object().unwrap().is_empty());
    }

    #[test]
    fn hotspots_file_properties_has_hash() {
        let props = hotspots_file_properties(Path::new("/src/bar.rs"));
        assert!(props.get("file-hash").is_some());
    }

    #[test]
    fn ownership_properties_has_hash() {
        let props = ownership_properties(42, Path::new("/src/baz.rs"));
        assert!(props.get("path-hash").is_some());
    }

    #[test]
    fn config_properties_get() {
        let props = config_properties(ConfigAction::Get, "access_token");
        assert_eq!(props["action"], json!("get"));
        assert_eq!(props["key"], json!("access_token"));
    }

    #[test]
    fn config_properties_set() {
        let props = config_properties(ConfigAction::Set, "onprem_url");
        assert_eq!(props["action"], json!("set"));
        assert_eq!(props["key"], json!("onprem_url"));
    }

    // ---- count_verdicts ----

    #[test]
    fn count_verdicts_empty() {
        let counts = count_verdicts(&[]);
        assert!(counts.is_empty());
    }

    #[test]
    fn count_verdicts_mixed() {
        let results = vec![
            json!({"verdict": "improved"}),
            json!({"verdict": "degraded"}),
            json!({"verdict": "improved"}),
            json!({}), // no verdict
        ];
        let counts = count_verdicts(&results);
        assert_eq!(counts.get("improved"), Some(&2));
        assert_eq!(counts.get("degraded"), Some(&1));
        assert_eq!(counts.len(), 2);
    }

    // ---- categories_from_entries ----

    #[test]
    fn categories_from_entries_deduplicates_and_sorts() {
        let entries = vec![
            json!({"category": "Complex Method"}),
            json!({"category": "Large Method"}),
            json!({"category": "Complex Method"}),
            json!({"no_category": true}),
        ];
        let refs: Vec<&Value> = entries.iter().collect();
        let cats = categories_from_entries(&refs);
        assert_eq!(cats, vec!["Complex Method", "Large Method"]);
    }

    // ---- add_categories ----

    #[test]
    fn add_categories_does_nothing_for_empty() {
        let mut props = json!({});
        add_categories(&mut props, &[]);
        assert!(props.get("categories").is_none());
    }

    #[test]
    fn add_categories_adds_when_non_empty() {
        let mut props = json!({});
        add_categories(&mut props, &["A".to_string(), "B".to_string()]);
        assert_eq!(props["categories"], json!(["A", "B"]));
        assert_eq!(props["category-count"], json!(2));
    }

    // ---- merge_delta_properties ----

    #[test]
    fn merge_delta_properties_no_results_key() {
        let mut props = json!({});
        merge_delta_properties(&mut props, r#"{"quality_gates":"passed"}"#);
        assert_eq!(props["quality-gates"], json!("passed"));
        assert!(props.get("file-count").is_none());
    }

    #[test]
    fn merge_delta_properties_with_results_gates_and_count() {
        let mut props = json!({});
        let data = r#"{"quality_gates":"failed","results":[{"verdict":"degraded","findings":[{"category":"Bumpy Road"}]}]}"#;
        merge_delta_properties(&mut props, data);
        assert_eq!(props["quality-gates"], json!("failed"));
        assert_eq!(props["file-count"], json!(1));
    }

    #[test]
    fn merge_delta_properties_with_results_verdicts_and_categories() {
        let mut props = json!({});
        let data = r#"{"quality_gates":"failed","results":[{"verdict":"degraded","findings":[{"category":"Bumpy Road"}]}]}"#;
        merge_delta_properties(&mut props, data);
        assert_eq!(props["verdicts"], json!({"degraded": 1}));
        assert_eq!(props["categories"], json!(["Bumpy Road"]));
    }
}
