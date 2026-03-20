/// Delta analysis parsing — mirrors Python's `delta_analysis.py`.
///
/// Parses the JSON array output from `cs delta` into structured results
/// with per-file verdicts and an overall quality gate status.

use serde::Serialize;
use serde_json::Value;

/// Overall result of a delta analysis.
#[derive(Debug, Clone, Serialize)]
pub struct DeltaResult {
    pub results: Vec<FileResult>,
    pub quality_gates: String,
}

/// Per-file delta result.
#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    pub name: String,
    pub verdict: String,
    pub findings: Vec<Value>,
}

/// Parse the raw output from `cs delta` into a `DeltaResult`.
///
/// The CLI emits a JSON array of file objects directly. Empty output
/// (no code health impact) maps to an empty result with "passed" gates.
pub fn analyze_delta_output(output: &str) -> DeltaResult {
    if output.trim().is_empty() {
        return DeltaResult {
            results: vec![],
            quality_gates: "passed".to_string(),
        };
    }

    let files: Vec<Value> = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => {
            return DeltaResult {
                results: vec![],
                quality_gates: "passed".to_string(),
            };
        }
    };

    let results: Vec<FileResult> = files.iter().map(build_file_result).collect();
    let has_degraded = results.iter().any(|r| r.verdict == "degraded");
    let quality_gates = if has_degraded { "failed" } else { "passed" };

    DeltaResult {
        results,
        quality_gates: quality_gates.to_string(),
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
///
/// Mirrors Python's `_get_verdict()`:
/// - new file (no old-score, has new-score) → "degraded"
/// - missing either score → "unknown"
/// - new > old → "improved", new < old → "degraded", equal → "stable"
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

/// Extract a numeric value from a JSON value (handles both int and float).
fn as_numeric(v: &Value) -> Option<f64> {
    v.as_f64()
}
