from .s_curve import metrics

# This module estimates the outcome of improving Code Health in terms of 
# faster development time and fewer defects.
#
# The theoretical background and research is 
# summarized here: https://codescene.com/blog/benchmarking-code-health-refactoring-roi
#
# We model three different scenarios:
TARGET_SCENARIOS = [
    {'scenario': 'Improve Code Health to the industry average.',
     'target_code_health': 5.15},
     {'scenario': 'Improve Code Health to the level of top 5% performers.',
      'target_code_health': 9.1},
    {'scenario': 'Maximize your speed and quality with a perfect Code Health score.',
     'target_code_health': 10.0}
]

def _outcome_of_refactoring_from(current_code_health):
    """
    Estimates the refactoring outcome in terms of development speed and defect reduction.

    We do this by finding the closest scenario based on the current_code_health. That is, 
    if you're below the industry average, then elevating your Code Healt to that level 
    should be the next step. Once there, you should aim for the top performers, etc. 
    """
    cloest_scenario = next(
        (s for s in TARGET_SCENARIOS if current_code_health < s["target_code_health"]),
        None
    )

    if cloest_scenario:
        return _build_business_case_for(cloest_scenario, current_code_health)
        
    return f'Your Code Health of {current_code_health} is already perfect. Keep up the good!'

def _build_business_case_for(scenario, current_code_health):
    estimated_outcomes = metrics.collect(code_health_baseline=current_code_health, code_health_target=scenario['target_code_health'])

    (defect_optimistic, defect_pessimistic) = estimated_outcomes['defects']
    (time_optimistic, time_pessimistic) = estimated_outcomes['time']

    def to_presentable(v):
        return abs(int(round(v)))

    estimate = {'title': 'Business case to motivate refactoring for improved Code Health',
                'data_description': '''
                The business case models two scenario: optimistic and pessimistic.
                CodeScene's model predicts that there's a 90 percent chance that the
                actual measured outcome falls in this range.
                ''',
                'current_code_health': current_code_health,
                'optimistic_outcome': {'defect_reduction':              to_presentable(defect_optimistic),
                                       'development_speed_improvement': to_presentable(time_optimistic)},
                'pessimistic_outcome': {'defect_reduction':             to_presentable(defect_pessimistic),
                                       'development_speed_improvement': to_presentable(time_pessimistic)}}

    return estimate | scenario

## Basic input validation

def _is_number(value):
    return isinstance(value, (int, float))

def _validate_code_health_score(name, value):
    """Validate that value is int/float and between 1.0 and 10.0 inclusive.

    Raises ValueError with a helpful message when invalid.
    """
    if not _is_number(value):
        raise ValueError(
            f"{name} must be an int or float between 1.0 and 10.0 (got {type(value).__name__})"
        )
    if value < 1.0 or value > 10.0:
        raise ValueError(f"{name} must be between 1.0 and 10.0 inclusive (got {value})")

def make_business_case_for(current_code_health):
    """Create a minimal business case for refactoring.
    """
    _validate_code_health_score("current_code_health", current_code_health)

    return {'outcome': _outcome_of_refactoring_from(current_code_health)}