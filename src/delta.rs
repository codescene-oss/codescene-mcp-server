use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct DeltaResult {
    pub results: Vec<FileResult>,
    pub quality_gates: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    pub name: String,
    pub verdict: String,
    pub findings: Vec<Value>,
}

pub fn analyze_delta_output(output: &str) -> DeltaResult {
    if output.trim().is_empty() {
        return empty_delta_result();
    }

    let parsed: Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return empty_delta_result(),
    };

    let Value::Object(map) = parsed else {
        return empty_delta_result();
    };

    let metadata = map.get("metadata").cloned();
    let files = map
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let results: Vec<FileResult> = files.iter().map(build_file_result).collect();
    let has_degraded = results.iter().any(|r| r.verdict == "degraded");
    let quality_gates = if has_degraded { "failed" } else { "passed" };

    DeltaResult {
        results,
        quality_gates: quality_gates.to_string(),
        metadata,
    }
}

fn empty_delta_result() -> DeltaResult {
    DeltaResult {
        results: vec![],
        quality_gates: "passed".to_string(),
        metadata: None,
    }
}

fn build_file_result(file: &Value) -> FileResult {
    let name = file
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let findings = file
        .get("findings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let verdict = compute_verdict(file);

    FileResult {
        name,
        verdict,
        findings,
    }
}

/// Determine the verdict for a single file based on score changes.
fn compute_verdict(file: &Value) -> String {
    let old_score = file.get("old-score").and_then(as_numeric);
    let new_score = file.get("new-score").and_then(as_numeric);

    match (old_score, new_score) {
        (None, Some(_)) => "degraded".to_string(),
        (None, None) | (Some(_), None) => "unknown".to_string(),
        (Some(old), Some(new)) => {
            if new > old {
                "improved".to_string()
            } else if new < old {
                "degraded".to_string()
            } else {
                "stable".to_string()
            }
        }
    }
}

fn as_numeric(v: &Value) -> Option<f64> {
    v.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper: build a single-file delta input and return the analyzed result.
    fn analyze_single_file(file_json: serde_json::Value) -> DeltaResult {
        let input = json!({"results": [file_json]});
        analyze_delta_output(&input.to_string())
    }

    fn assert_empty_passed_without_metadata(result: &DeltaResult) {
        assert!(result.results.is_empty());
        assert_eq!(result.quality_gates, "passed");
        assert!(result.metadata.is_none());
    }

    fn assert_empty_passed_with_metadata_status(result: &DeltaResult, expected_status: &str) {
        assert!(result.results.is_empty());
        assert_eq!(result.quality_gates, "passed");
        assert_eq!(
            result
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("status"))
                .and_then(|status| status.as_str()),
            Some(expected_status)
        );
    }

    // ---- analyze_delta_output ----

    #[test]
    fn empty_input_returns_passed() {
        let r = analyze_delta_output("");
        assert_empty_passed_without_metadata(&r);
    }

    #[test]
    fn whitespace_only_returns_passed() {
        let r = analyze_delta_output("   \n  ");
        assert!(r.results.is_empty());
        assert_eq!(r.quality_gates, "passed");
    }

    #[test]
    fn invalid_json_returns_passed() {
        let r = analyze_delta_output("not json at all");
        assert_empty_passed_without_metadata(&r);
    }

    #[test]
    fn non_object_json_returns_passed() {
        let r = analyze_delta_output("[]");
        assert_empty_passed_without_metadata(&r);
    }

    #[test]
    fn metadata_is_preserved_for_clean_empty_results() {
        let input = json!({
            "metadata": {
                "status": "no-issues-found",
                "total-modified-file-count": 2,
                "code-health-eligible-file-count": 1,
                "checked-file-count": 1
            },
            "results": []
        });
        let r = analyze_delta_output(&input.to_string());
        assert_empty_passed_with_metadata_status(&r, "no-issues-found");
    }

    #[test]
    fn metadata_is_preserved_for_no_modified_files() {
        let input = json!({
            "metadata": {
                "status": "no-files-modified",
                "total-modified-file-count": 0,
                "code-health-eligible-file-count": 0,
                "checked-file-count": 0
            },
            "results": []
        });
        let r = analyze_delta_output(&input.to_string());
        assert_empty_passed_with_metadata_status(&r, "no-files-modified");
    }

    #[test]
    fn object_without_results_defaults_to_empty_results() {
        let input = json!({"metadata": {"status": "no-files-modified"}});
        let r = analyze_delta_output(&input.to_string());
        assert_empty_passed_with_metadata_status(&r, "no-files-modified");
    }

    #[test]
    fn single_stable_file() {
        let input = json!({
            "results": [{
                "name": "foo.rs",
                "old-score": 8.0,
                "new-score": 8.0,
                "findings": [{"category": "Complex Method"}]
            }]
        });
        let r = analyze_delta_output(&input.to_string());
        assert_eq!(r.results.len(), 1);
        assert_eq!(r.results[0].name, "foo.rs");
        assert_eq!(r.results[0].verdict, "stable");
        assert_eq!(r.results[0].findings.len(), 1);
        assert_eq!(r.quality_gates, "passed");
    }

    #[test]
    fn degraded_file_fails_quality_gates() {
        let r = analyze_single_file(json!({
            "name": "bar.rs", "old-score": 9.0, "new-score": 7.0, "findings": []
        }));
        assert_eq!(r.results[0].verdict, "degraded");
        assert_eq!(r.quality_gates, "failed");
    }

    #[test]
    fn improved_file_passes() {
        let r = analyze_single_file(json!({
            "name": "baz.rs", "old-score": 5.0, "new-score": 9.0, "findings": []
        }));
        assert_eq!(r.results[0].verdict, "improved");
        assert_eq!(r.quality_gates, "passed");
    }

    #[test]
    fn new_file_is_degraded() {
        let r = analyze_single_file(json!({
            "name": "new.rs", "new-score": 6.0, "findings": []
        }));
        assert_eq!(r.results[0].verdict, "degraded");
        assert_eq!(r.quality_gates, "failed");
    }

    #[test]
    fn missing_both_scores_is_unknown() {
        let r = analyze_single_file(json!({
            "name": "mystery.rs", "findings": []
        }));
        assert_eq!(r.results[0].verdict, "unknown");
        assert_eq!(r.quality_gates, "passed");
    }

    #[test]
    fn old_score_only_is_unknown() {
        let r = analyze_single_file(json!({
            "name": "old.rs", "old-score": 5.0, "findings": []
        }));
        assert_eq!(r.results[0].verdict, "unknown");
    }

    #[test]
    fn mixed_verdicts_one_degraded_fails() {
        let input = json!({
            "results": [
                {"name": "a.rs", "old-score": 8.0, "new-score": 9.0, "findings": []},
                {"name": "b.rs", "old-score": 9.0, "new-score": 7.0, "findings": []}
            ]
        });
        let r = analyze_delta_output(&input.to_string());
        assert_eq!(r.results.len(), 2);
        assert_eq!(r.results[0].verdict, "improved");
        assert_eq!(r.results[1].verdict, "degraded");
        assert_eq!(r.quality_gates, "failed");
    }

    #[test]
    fn missing_name_defaults_to_unknown() {
        let input = json!({"results": [{"old-score": 5.0, "new-score": 5.0}]});
        let r = analyze_delta_output(&input.to_string());
        assert_eq!(r.results[0].name, "unknown");
    }

    #[test]
    fn missing_findings_defaults_to_empty() {
        let input = json!({"results": [{"name": "x.rs", "old-score": 5.0, "new-score": 5.0}]});
        let r = analyze_delta_output(&input.to_string());
        assert!(r.results[0].findings.is_empty());
    }

    #[test]
    fn integer_scores_work() {
        let input = json!({"results": [{"name": "int.rs", "old-score": 5, "new-score": 8, "findings": []}]});
        let r = analyze_delta_output(&input.to_string());
        assert_eq!(r.results[0].verdict, "improved");
    }

    // ---- DeltaResult serialisation ----

    #[test]
    fn delta_result_serializes() {
        let r = DeltaResult {
            results: vec![FileResult {
                name: "test.rs".into(),
                verdict: "stable".into(),
                findings: vec![json!({"cat": "a"})],
            }],
            quality_gates: "passed".into(),
            metadata: Some(json!({"status": "no-issues-found"})),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"quality_gates\":\"passed\""));
        assert!(json.contains("\"verdict\":\"stable\""));
        assert!(json.contains("\"metadata\":{\"status\":\"no-issues-found\"}"));
    }
}
