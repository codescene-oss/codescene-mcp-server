import json

class DeltaAnalysisError(Exception):
    pass

def analyze_delta_output(output: str):
    """
    Analyze the delta output from CodeScene CLI and return per-file verdicts and overall verdict.

    Args:
        output (str): JSON string from CodeScene CLI.

    Returns:
        dict: {
            "results": [...],
            "quality_gates": "passed" or "failed"
        }
    """
    if _no_code_health_impact(output):
        # No issues, quality gates pass
        return {
            "results": [],
            "quality_gates": "passed"
        }
    files_with_ch_impact = _parse_json_output(output)
    return _outcome_depends_on(files_with_ch_impact)

def _no_code_health_impact(output: str):
    return not output.strip()

def _outcome_depends_on(files_with_ch_impact):
    results, has_degraded = _evaluate_files(files_with_ch_impact)
    quality_gates = _determine_quality_gates(has_degraded)
    return {
        "results": results,
        "quality_gates": quality_gates
    }

def _parse_json_output(output: str):
    try:
        return json.loads(output)
    except Exception as e:
        raise DeltaAnalysisError(
            f"Invalid JSON input: {e}\nInput: {output[:500]}"
        )

def _evaluate_files(files):
    results = [
        {
            "name": file.get("name"),
            "verdict":  _get_verdict(file),
            "findings": file.get("findings", [])
        }
        for file in files
    ]
    has_degraded = any(r["verdict"] == "degraded" for r in results)
    return results, has_degraded

def _get_verdict(file):
    old_score, new_score = file.get("old-score"), file.get("new-score")
    if old_score is None or new_score is None:
        return "unknown"
    return (
        "improved" if new_score > old_score else
        "degraded" if new_score < old_score else
        "stable"
    )

def _determine_quality_gates(has_degraded):
    return "failed" if has_degraded else "passed"