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

    def test_outcome_for_unhealthy_code(self):
        the_case = self._outcome_for(current_code_health=3.9)

        self.assertEqual({
             'title': 'Business case to motivate refactoring for improved Code Health', 
             'current_code_health': 3.9, 
             'optimistic_outcome': {
                  'defect_reduction': 27.33, 
                  'development_speed_improvement': 11.89}, 
             'pessimistic_outcome': {
                  'defect_reduction': 10.66, 
                  'development_speed_improvement': 1.91}, 
             'scenario': 'Improve Code Health to the industry average.', 'target_code_health': 5.15},
             the_case)
    
    def test_problematic_code_above_industry_baseline(self):
        the_case = self._outcome_for(current_code_health=5.2)
        
        self.assertEqual({
            'title': 'Business case to motivate refactoring for improved Code Health', 
            'current_code_health': 5.2, 
            'optimistic_outcome': {
                'defect_reduction': 49.41, 
                'development_speed_improvement': 42.78}, 
            'pessimistic_outcome': {
                'defect_reduction': 31.63, 
                'development_speed_improvement': 26.81}, 
            'scenario': 'Improve Code Health to the level of top 5% performers.', 
            'target_code_health': 9.1},
             the_case)
    
    def test_healthy_top_performers_code(self):
        the_case = self._outcome_for(current_code_health=9.3)
        
        self.assertEqual({
            'title': 'Business case to motivate refactoring for improved Code Health', 
            'current_code_health': 9.3, 
            'optimistic_outcome': {
                'defect_reduction': 58.91, 
                'development_speed_improvement': 26.9}, 
            'pessimistic_outcome': {
                'defect_reduction': 47.94, 
                'development_speed_improvement': 12.73}, 
            'scenario': 'Maximize your speed and quality with a perfect Code Health score.', 
            'target_code_health': 10.0},
             the_case)
    
    def test_perfect_ten_code(self):
        the_case = make_business_case_for(current_code_health=10.0)
        outcome = the_case['outcome']

        self.assertEqual('Your Code Health of 10.0 is already perfect. Keep up the good!', outcome)

    def _outcome_for(self, current_code_health):
        the_case = make_business_case_for(current_code_health)
        outcome = the_case['outcome']
        outcome.pop('data_description') # data_description is a verbose static string intended for the LLMs
        return outcome


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
