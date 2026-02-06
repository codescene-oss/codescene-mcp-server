import json
import unittest

from code_health_tools.delta_analysis import DeltaAnalysisError, analyze_delta_output


def make_delta_json(*files):
    entries = [build_file_entry(**f) for f in files]
    cleaned_entries = [remove_none_fields(e) for e in entries]
    return json.dumps(cleaned_entries)


def build_file_entry(name, old_score=None, new_score=None, findings=None):
    return {
        "name": name,
        "old-score": old_score if old_score is not None else None,
        "new-score": new_score if new_score is not None else None,
        "findings": findings if findings is not None else [],
    }


def remove_none_fields(entry):
    return {k: v for k, v in entry.items() if v is not None or k == "findings"}


class TestDeltaAnalysis(unittest.TestCase):
    def test_code_health_improved(self):
        output = make_delta_json({"name": "file1.cpp", "old_score": 7.0, "new_score": 8.0})
        result = analyze_delta_output(output)
        self.assertCodeHealthImproved(result)
        self.assertQualityGatesPass(result)

    def test_code_health_degraded(self):
        output = make_delta_json({"name": "file2.cpp", "old_score": 8.0, "new_score": 7.0})
        result = analyze_delta_output(output)
        self.assertCodeHealthDegraded(result)
        self.assertQualityGatesFail(result)

    def test_code_health_stable(self):
        output = make_delta_json({"name": "file3.cpp", "old_score": 8.0, "new_score": 8.0})
        result = analyze_delta_output(output)
        self.assertCodeHealthStable(result)
        self.assertQualityGatesPass(result)

    def test_code_health_mixed(self):
        output = make_delta_json(
            {"name": "file1.cpp", "old_score": 7.0, "new_score": 8.0},
            {"name": "file2.cpp", "old_score": 8.0, "new_score": 7.0},
        )
        result = analyze_delta_output(output)
        self.assertCodeHealthImproved(result, filename="file1.cpp")
        self.assertCodeHealthDegraded(result, filename="file2.cpp")
        self.assertQualityGatesFail(result)

    def test_code_health_unknown(self):
        output = make_delta_json({"name": "file4.cpp"})
        result = analyze_delta_output(output)
        self.assertCodeHealthUnknown(result)
        self.assertQualityGatesPass(result)

    def test_invalid_json(self):
        with self.assertRaises(DeltaAnalysisError):
            analyze_delta_output("not a json string")

    def test_empty_input_quality_gates_pass(self):
        result = analyze_delta_output("")
        self.assertEqual(result["results"], [])
        self.assertQualityGatesPass(result)

    # Clarify the intent via custom assertions:
    #
    def assertCodeHealthImproved(self, result, filename=None):
        entry = self._result_matching(filename, result)
        self.assertIsNotNone(entry, f"No result entry found for filename: {filename}")
        self.assertEqual(entry["verdict"], "improved")

    def assertCodeHealthDegraded(self, result, filename=None):
        entry = self._result_matching(filename, result)
        self.assertIsNotNone(entry, f"No result entry found for filename: {filename}")
        self.assertEqual(entry["verdict"], "degraded")

    def assertCodeHealthStable(self, result, filename=None):
        entry = self._result_matching(filename, result)
        self.assertIsNotNone(entry, f"No result entry found for filename: {filename}")
        self.assertEqual(entry["verdict"], "stable")

    def assertCodeHealthUnknown(self, result, filename=None):
        entry = self._result_matching(filename, result)
        self.assertIsNotNone(entry, f"No result entry found for filename: {filename}")
        self.assertEqual(entry["verdict"], "unknown")

    def assertQualityGatesPass(self, result):
        self.assertEqual(result["quality_gates"], "passed")

    def assertQualityGatesFail(self, result):
        self.assertEqual(result["quality_gates"], "failed")

    def _result_matching(self, filename, result):
        return next(
            (e for e in result["results"] if filename is None or e.get("name") == filename),
            None,
        )


if __name__ == "__main__":
    unittest.main()
