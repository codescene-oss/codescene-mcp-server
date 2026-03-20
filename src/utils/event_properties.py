"""Tool-specific property extractors for analytics events.

Each extractor receives the tool's return value and the same positional /
keyword arguments that the tool method was called with, then returns a dict
of event properties to merge into the tracking payload.

Extractors must never raise — the ``@track`` decorator wraps calls in a
blanket ``try/except`` so a broken extractor cannot disrupt a tool response.
"""

import json
import re

from utils.hashing import hash_value


# -- CLI-based tools --


def _parse_json_dict(result) -> dict | None:
    """Parse *result* as JSON and return it if it's a dict, else ``None``."""
    try:
        data = json.loads(result)
        return data if isinstance(data, dict) else None
    except (json.JSONDecodeError, TypeError):
        return None


def _categories_from_entries(entries: list) -> list[str]:
    """Return sorted unique category strings from a list of dict entries."""
    return sorted({
        e["category"] for e in entries
        if isinstance(e, dict) and "category" in e
    })


def _add_categories(props: dict, categories: list[str]) -> None:
    """Add categories and category-count to *props* if non-empty."""
    if categories:
        props["categories"] = categories
        props["category-count"] = len(categories)


def _count_verdicts(results: list[dict]) -> dict[str, int]:
    """Count verdict occurrences across delta results entries."""
    verdicts: dict[str, int] = {}
    for entry in results:
        v = entry.get("verdict") if isinstance(entry, dict) else None
        if v:
            verdicts[v] = verdicts.get(v, 0) + 1
    return verdicts


def code_health_score_properties(result, file_path: str, **_kw) -> dict:
    """Extract the numeric score and a file-path hash."""
    props: dict = {"file-hash": hash_value(file_path)}
    match = re.search(r"score:\s*([\d.]+)", str(result))
    if match:
        props["score"] = match.group(1)
    return props


def code_health_review_properties(result, file_path: str, **_kw) -> dict:
    """Extract file hash, score, and code smell categories from a review."""
    props: dict = {"file-hash": hash_value(file_path)}
    data = _parse_json_dict(result)
    if data is None:
        return props
    if "score" in data:
        props["score"] = data["score"]
    review = data.get("review")
    if isinstance(review, list):
        _add_categories(props, _categories_from_entries(review))
    return props


def _extract_delta_properties(result) -> dict:
    """Extract non-PII metadata from a delta analysis result (shared helper).

    Works for both pre-commit safeguard and analyze-change-set, which return
    the same ``{"quality_gates": ..., "results": [...]}`` JSON structure.
    """
    data = _parse_json_dict(result)
    if data is None:
        return {}
    props: dict = {}
    if "quality_gates" in data:
        props["quality-gates"] = data["quality_gates"]
    results = data.get("results")
    if not isinstance(results, list):
        return props
    props["file-count"] = len(results)
    verdicts = _count_verdicts(results)
    if verdicts:
        props["verdicts"] = verdicts
    findings = [f for entry in results if isinstance(entry, dict) for f in entry.get("findings", [])]
    _add_categories(props, _categories_from_entries(findings))
    return props


def pre_commit_properties(result, git_repository_path: str, **_kw) -> dict:
    """Extract repo hash and delta analysis metadata from a pre-commit safeguard call."""
    props: dict = {"repo-hash": hash_value(git_repository_path)}
    props.update(_extract_delta_properties(result))
    return props


def analyze_change_set_properties(result, base_ref: str, git_repository_path: str, **_kw) -> dict:
    """Extract hashed repo/ref and delta analysis metadata from an analyze-change-set call."""
    props: dict = {
        "repo-hash": hash_value(git_repository_path),
        "base-ref-hash": hash_value(base_ref),
    }
    props.update(_extract_delta_properties(result))
    return props


def auto_refactor_properties(result, file_path: str, function_name: str, **_kw) -> dict:
    """Extract file hash and ACE confidence from an auto-refactor call."""
    props: dict = {"file-hash": hash_value(file_path)}
    data = _parse_json_dict(result)
    if data is not None and "confidence" in data:
        props["confidence"] = data["confidence"]
    return props


def business_case_properties(result, file_path: str, **_kw) -> dict:
    """Extract file hash and business-case metadata (current score, target)."""
    props: dict = {"file-hash": hash_value(file_path)}
    data = _parse_json_dict(result)
    if data is None:
        return props
    outcome = data.get("outcome")
    if not isinstance(outcome, dict):
        return props
    if "current_code_health" in outcome:
        props["current-code-health"] = outcome["current_code_health"]
    if "target_code_health" in outcome:
        props["target-code-health"] = outcome["target_code_health"]
    return props


# -- API-based tools --


def select_project_properties(result, **_kw) -> dict:
    """No tool-specific args to hash — return empty."""
    return {}


def hotspots_for_project_properties(result, project_id: int, **_kw) -> dict:
    return {}


def hotspots_for_file_properties(result, file_path: str, project_id: int, **_kw) -> dict:
    return {"file-hash": hash_value(file_path)}


def goals_for_project_properties(result, project_id: int, **_kw) -> dict:
    return {}


def goals_for_file_properties(result, file_path: str, project_id: int, **_kw) -> dict:
    return {"file-hash": hash_value(file_path)}


def code_ownership_properties(result, project_id: int, path: str, **_kw) -> dict:
    return {"path-hash": hash_value(path)}
