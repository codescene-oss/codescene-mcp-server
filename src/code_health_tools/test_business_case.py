import unittest

from code_health_tools.business_case import make_business_case_for

class TestMakeBusinessCaseFor(unittest.TestCase):
	
    VALID_CASES = [
        ("boundary_min",     1.0),
        ("boundary_max",    10.0),
        ("mid_value",        5)
    ]

    def test_valid_code_health_range(self):
        for name, current in self.VALID_CASES:
            res = make_business_case_for(current)
            self.assertIn("outcome", res, name)

    ERRONEOUS_CASES = [
		("below_min",    0.999),
		("above_max",    10.0001),
		("non_numeric", "a")
	]

    def test_invalid_input_raises(self):
        for _name, current in self.ERRONEOUS_CASES:
            with self.assertRaises(ValueError):
                make_business_case_for(current)

if __name__ == "__main__":
	unittest.main()
