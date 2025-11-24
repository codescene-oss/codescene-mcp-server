import unittest
import json
from code_health_tools.delta_analysis import analyze_delta_output, DeltaAnalysisError

def make_delta_json(*files):
    entries = [build_file_entry(**f) for f in files]
    cleaned_entries = [remove_none_fields(e) for e in entries]
    return json.dumps(cleaned_entries)

def build_file_entry(name, old_score=None, new_score=None, findings=None):
    return {
        "name": name,
        "old-score": old_score if old_score is not None else None,
        "new-score": new_score if new_score is not None else None,
        "findings":  findings if findings is not None else []
    }

def remove_none_fields(entry):
    return {k: v for k, v in entry.items() if v is not None or k == "findings"}

class TestDeltaAnalysis(unittest.TestCase):
    
    def test_code_health_improved(self):
        output = make_delta_json(
            {"name": "file1.cpp", "old_score": 7.0, "new_score": 8.0}
        )
        result = analyze_delta_output(output)
        self.assertCodeHealthImproved(result)
        self.assertQualityGatesPass(result)

    def test_code_health_degraded(self):
        output = make_delta_json(
            {"name": "file2.cpp", "old_score": 8.0, "new_score": 7.0}
        )
        result = analyze_delta_output(output)
        self.assertCodeHealthDegraded(result)
        self.assertQualityGatesFail(result)

    def test_code_health_stable(self):
        output = make_delta_json(
            {"name": "file3.cpp", "old_score": 8.0, "new_score": 8.0}
        )
        result = analyze_delta_output(output)
        self.assertCodeHealthStable(result)
        self.assertQualityGatesPass(result)

    def test_code_health_mixed(self):
        output = make_delta_json(
            {"name": "file1.cpp", "old_score": 7.0, "new_score": 8.0},
            {"name": "file2.cpp", "old_score": 8.0, "new_score": 7.0}
        )
        result = analyze_delta_output(output)
        self.assertCodeHealthImproved(result, idx=0)
        self.assertCodeHealthDegraded(result, idx=1)
        self.assertQualityGatesFail(result)

    def test_code_health_unknown(self):
        output = make_delta_json(
            {"name": "file4.cpp"}
        )
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
    def assertCodeHealthImproved(self, result, idx=0):
        self.assertEqual(result["results"][idx]["verdict"], "improved")

    def assertCodeHealthDegraded(self, result, idx=0):
        self.assertEqual(result["results"][idx]["verdict"], "degraded")

    def assertCodeHealthStable(self, result, idx=0):
        self.assertEqual(result["results"][idx]["verdict"], "stable")

    def assertCodeHealthUnknown(self, result, idx=0):
        self.assertEqual(result["results"][idx]["verdict"], "unknown")

    def assertQualityGatesPass(self, result):
        self.assertEqual(result["quality_gates"], "passed")

    def assertQualityGatesFail(self, result):
        self.assertEqual(result["quality_gates"], "failed")

if __name__ == "__main__":
    unittest.main()