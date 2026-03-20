"""Unit tests for tool-specific property extractors in event_properties.py."""

import json
import unittest

from utils.event_properties import (
    analyze_change_set_properties,
    auto_refactor_properties,
    business_case_properties,
    code_health_review_properties,
    code_health_score_properties,
    code_ownership_properties,
    goals_for_file_properties,
    goals_for_project_properties,
    hotspots_for_file_properties,
    hotspots_for_project_properties,
    pre_commit_properties,
    select_project_properties,
)
from utils.hashing import hash_value


# -- Shared fixture builders --


def _delta_result(gate: str, files: list[dict]) -> str:
    """Build a JSON delta-analysis result string."""
    return json.dumps({"quality_gates": gate, "results": files})


def _file_entry(name: str, verdict: str, categories: list[str] | None = None) -> dict:
    """Build a single file entry for delta results."""
    findings = [{"category": c, "change-type": "introduced"} for c in (categories or [])]
    return {"name": name, "verdict": verdict, "findings": findings}


def _review_result(score: float, categories: list[str]) -> str:
    """Build a JSON review result string with the given categories."""
    review = [{"category": c, "indication": 2} for c in categories]
    return json.dumps({"score": score, "review": review})


# -- Shared assertion helpers --


def _assert_categories(test: unittest.TestCase, props: dict, expected: list[str]) -> None:
    """Assert that props contain the expected sorted categories and count."""
    test.assertEqual(props["categories"], expected)
    test.assertEqual(props["category-count"], len(expected))


def _assert_no_categories(test: unittest.TestCase, props: dict) -> None:
    """Assert that props contain no category fields."""
    test.assertNotIn("categories", props)
    test.assertNotIn("category-count", props)


class TestCodeHealthScoreProperties(unittest.TestCase):
    def test_extracts_score_and_file_hash(self):
        result = "Code Health score: 9.5"
        props = code_health_score_properties(result, file_path="/repo/src/module.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/src/module.py"))
        self.assertEqual(props["score"], "9.5")

    def test_extracts_integer_score(self):
        result = "Code Health score: 10.0"
        props = code_health_score_properties(result, file_path="/repo/app.py")
        self.assertEqual(props["score"], "10.0")

    def test_missing_score_still_returns_file_hash(self):
        result = "Error: file not found"
        props = code_health_score_properties(result, file_path="/repo/missing.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/missing.py"))
        self.assertNotIn("score", props)

    def test_hash_is_16_chars(self):
        props = code_health_score_properties("score: 5.0", file_path="/any/path.py")
        self.assertEqual(len(props["file-hash"]), 16)


class TestCodeHealthReviewProperties(unittest.TestCase):
    def test_extracts_file_hash(self):
        props = code_health_review_properties("review output", file_path="/repo/src/foo.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/src/foo.py"))

    def test_non_json_result_returns_only_file_hash(self):
        props = code_health_review_properties("plain string output", file_path="/repo/bar.py")
        self.assertEqual(list(props.keys()), ["file-hash"])

    def test_extracts_score_from_json_review(self):
        result = json.dumps({"score": 7.5, "review": []})
        props = code_health_review_properties(result, file_path="/repo/f.py")
        self.assertEqual(props["score"], 7.5)

    def test_extracts_categories_from_json_review(self):
        result = _review_result(3.2, ["Complex Method", "Bumpy Road Ahead", "Large Method"])
        props = code_health_review_properties(result, file_path="/repo/f.py")
        _assert_categories(self, props, ["Bumpy Road Ahead", "Complex Method", "Large Method"])

    def test_categories_are_deduplicated_and_sorted(self):
        result = json.dumps({
            "score": 5.0,
            "review": [
                {"category": "Large Method", "indication": 2},
                {"category": "Complex Method", "indication": 2},
                {"category": "Large Method", "indication": 3},
            ],
        })
        props = code_health_review_properties(result, file_path="/repo/f.py")
        _assert_categories(self, props, ["Complex Method", "Large Method"])

    def test_empty_review_array_has_no_categories(self):
        result = json.dumps({"score": 10.0, "review": []})
        props = code_health_review_properties(result, file_path="/repo/f.py")
        _assert_no_categories(self, props)
        self.assertEqual(props["score"], 10.0)

    def test_json_without_review_key_returns_score_only(self):
        result = json.dumps({"score": 8.0})
        props = code_health_review_properties(result, file_path="/repo/f.py")
        self.assertEqual(props["score"], 8.0)
        _assert_no_categories(self, props)

    def test_json_array_result_returns_only_file_hash(self):
        result = json.dumps([{"category": "Complex Method"}])
        props = code_health_review_properties(result, file_path="/repo/f.py")
        self.assertEqual(list(props.keys()), ["file-hash"])

    def test_review_entries_without_category_are_skipped(self):
        result = json.dumps({
            "score": 6.0,
            "review": [
                {"category": "Complex Method", "indication": 2},
                {"description": "some entry without category"},
                "unexpected string entry",
            ],
        })
        props = code_health_review_properties(result, file_path="/repo/f.py")
        _assert_categories(self, props, ["Complex Method"])


class TestPreCommitProperties(unittest.TestCase):
    def test_extracts_repo_hash(self):
        props = pre_commit_properties("{}", git_repository_path="/repo")
        self.assertEqual(props["repo-hash"], hash_value("/repo"))

    def test_non_json_result_returns_only_repo_hash(self):
        props = pre_commit_properties("Error: not json", git_repository_path="/repo")
        self.assertEqual(list(props.keys()), ["repo-hash"])

    def test_extracts_quality_gates_passed(self):
        result = _delta_result("passed", [])
        props = pre_commit_properties(result, git_repository_path="/repo")
        self.assertEqual(props["quality-gates"], "passed")
        self.assertEqual(props["file-count"], 0)

    def test_extracts_quality_gates_failed_with_verdicts(self):
        result = _delta_result("failed", [
            _file_entry("a.py", "degraded", ["Complex Method"]),
            _file_entry("b.py", "improved"),
        ])
        props = pre_commit_properties(result, git_repository_path="/repo")
        self.assertEqual(props["quality-gates"], "failed")
        self.assertEqual(props["file-count"], 2)
        self.assertEqual(props["verdicts"], {"degraded": 1, "improved": 1})
        _assert_categories(self, props, ["Complex Method"])

    def test_extracts_categories_from_multiple_files(self):
        result = _delta_result("failed", [
            _file_entry("x.py", "degraded", ["Large Method", "Complex Conditional"]),
            _file_entry("y.py", "degraded", ["Large Method"]),
        ])
        props = pre_commit_properties(result, git_repository_path="/repo")
        _assert_categories(self, props, ["Complex Conditional", "Large Method"])

    def test_empty_results_has_no_verdicts_or_categories(self):
        result = _delta_result("passed", [])
        props = pre_commit_properties(result, git_repository_path="/repo")
        self.assertNotIn("verdicts", props)
        _assert_no_categories(self, props)


class TestAnalyzeChangeSetProperties(unittest.TestCase):
    def test_extracts_repo_and_base_ref_hashes(self):
        props = analyze_change_set_properties("{}", base_ref="main", git_repository_path="/repo")
        self.assertEqual(props["repo-hash"], hash_value("/repo"))
        self.assertEqual(props["base-ref-hash"], hash_value("main"))

    def test_non_json_result_returns_only_hashes(self):
        props = analyze_change_set_properties("Error: not json", base_ref="main", git_repository_path="/repo")
        self.assertEqual(sorted(props.keys()), ["base-ref-hash", "repo-hash"])

    def test_extracts_quality_gates_passed(self):
        result = _delta_result("passed", [])
        props = analyze_change_set_properties(result, base_ref="main", git_repository_path="/repo")
        self.assertEqual(props["quality-gates"], "passed")
        self.assertEqual(props["file-count"], 0)

    def test_extracts_quality_gates_failed_with_verdicts(self):
        result = _delta_result("failed", [
            _file_entry("a.py", "degraded", ["Complex Method"]),
            _file_entry("b.py", "stable"),
            _file_entry("c.py", "degraded", ["Large Method"]),
        ])
        props = analyze_change_set_properties(result, base_ref="develop", git_repository_path="/repo")
        self.assertEqual(props["quality-gates"], "failed")
        self.assertEqual(props["file-count"], 3)
        self.assertEqual(props["verdicts"], {"degraded": 2, "stable": 1})
        _assert_categories(self, props, ["Complex Method", "Large Method"])
        self.assertEqual(props["base-ref-hash"], hash_value("develop"))

    def test_extracts_categories_across_multiple_files(self):
        result = _delta_result("failed", [
            _file_entry("x.py", "degraded", ["Bumpy Road Ahead", "Complex Conditional"]),
            _file_entry("y.py", "degraded", ["Bumpy Road Ahead"]),
        ])
        props = analyze_change_set_properties(result, base_ref="main", git_repository_path="/repo")
        _assert_categories(self, props, ["Bumpy Road Ahead", "Complex Conditional"])

    def test_empty_results_has_no_verdicts_or_categories(self):
        result = _delta_result("passed", [])
        props = analyze_change_set_properties(result, base_ref="main", git_repository_path="/repo")
        self.assertNotIn("verdicts", props)
        _assert_no_categories(self, props)


class TestAutoRefactorProperties(unittest.TestCase):
    def test_extracts_file_hash_and_confidence(self):
        result = json.dumps({"code": "refactored", "confidence": "high"})
        props = auto_refactor_properties(result, file_path="/repo/service.ts", function_name="calculate")
        self.assertEqual(props["file-hash"], hash_value("/repo/service.ts"))
        self.assertEqual(props["confidence"], "high")

    def test_extracts_file_hash_when_result_is_error(self):
        props = auto_refactor_properties("Error: something failed", file_path="/repo/x.ts", function_name="fn")
        self.assertEqual(props["file-hash"], hash_value("/repo/x.ts"))
        self.assertNotIn("confidence", props)

    def test_extracts_file_hash_when_no_confidence_key(self):
        result = json.dumps({"code": "refactored"})
        props = auto_refactor_properties(result, file_path="/repo/y.ts", function_name="fn")
        self.assertEqual(props["file-hash"], hash_value("/repo/y.ts"))
        self.assertNotIn("confidence", props)


class TestBusinessCaseProperties(unittest.TestCase):
    def test_extracts_file_hash(self):
        props = business_case_properties("{}", file_path="/repo/legacy.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/legacy.py"))

    def test_extracts_current_and_target_code_health(self):
        result = json.dumps({
            "outcome": {
                "current_code_health": 3.5,
                "target_code_health": 5.15,
                "scenario": "If refactored to 5.15...",
                "optimistic_outcome": {},
                "pessimistic_outcome": {},
            }
        })
        props = business_case_properties(result, file_path="/repo/legacy.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/legacy.py"))
        self.assertEqual(props["current-code-health"], 3.5)
        self.assertEqual(props["target-code-health"], 5.15)

    def test_string_outcome_returns_only_file_hash(self):
        result = json.dumps({
            "outcome": "Code Health is already perfect — no refactoring needed."
        })
        props = business_case_properties(result, file_path="/repo/perfect.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/perfect.py"))
        self.assertNotIn("current-code-health", props)
        self.assertNotIn("target-code-health", props)

    def test_non_json_result_returns_only_file_hash(self):
        props = business_case_properties("Error: analysis failed", file_path="/repo/err.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/err.py"))
        self.assertNotIn("current-code-health", props)
        self.assertNotIn("target-code-health", props)

    def test_missing_outcome_returns_only_file_hash(self):
        result = json.dumps({"other_key": "value"})
        props = business_case_properties(result, file_path="/repo/x.py")
        self.assertEqual(props["file-hash"], hash_value("/repo/x.py"))
        self.assertNotIn("current-code-health", props)


class TestSelectProjectProperties(unittest.TestCase):
    def test_returns_empty_dict(self):
        props = select_project_properties("{}")
        self.assertEqual(props, {})


class TestHotspotsForProjectProperties(unittest.TestCase):
    def test_returns_empty_dict(self):
        props = hotspots_for_project_properties("{}", project_id=42)
        self.assertEqual(props, {})


class TestHotspotsForFileProperties(unittest.TestCase):
    def test_extracts_file_hash(self):
        props = hotspots_for_file_properties("{}", file_path="/repo/hot.py", project_id=42)
        self.assertEqual(props["file-hash"], hash_value("/repo/hot.py"))


class TestGoalsForProjectProperties(unittest.TestCase):
    def test_returns_empty_dict(self):
        props = goals_for_project_properties("{}", project_id=42)
        self.assertEqual(props, {})


class TestGoalsForFileProperties(unittest.TestCase):
    def test_extracts_file_hash(self):
        props = goals_for_file_properties("{}", file_path="/repo/goal.py", project_id=42)
        self.assertEqual(props["file-hash"], hash_value("/repo/goal.py"))


class TestCodeOwnershipProperties(unittest.TestCase):
    def test_extracts_path_hash(self):
        props = code_ownership_properties("[]", project_id=42, path="/repo/src/service.py")
        self.assertEqual(props["path-hash"], hash_value("/repo/src/service.py"))


if __name__ == "__main__":
    unittest.main()
