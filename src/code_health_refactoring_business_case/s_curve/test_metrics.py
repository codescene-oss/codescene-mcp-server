import unittest

from . import metrics


class TestScurveCalculation(unittest.TestCase):
    def test_estimates_defect_and_dev_time_improvements(self):
        result = metrics.collect(code_health_baseline=2.0, code_health_target=5.15)

        self.assertEqual(result["defects"], (-65.6, -39.37), "CI90 for defect reduction.")
        self.assertEqual(result["time"], (-29.18, -5.58), "CI90 for development time reduction.")

    def test_outcomes_improve_with_code_health(self):
        result = metrics.collect(code_health_baseline=2.0, code_health_target=10.0)
        print(result)
        self.assertEqual(result["defects"], (-91.24, -84.78), "CI90 for defect reduction.")
        self.assertEqual(result["time"], (-66.04, -52.3), "CI90 for development time reduction.")


if __name__ == "__main__":
    unittest.main()
