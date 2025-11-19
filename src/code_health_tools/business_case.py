from code_health_tools.s_curve import metrics

def _outcome_of_refactoring_from(current_code_health):
    """Estimates the refactoring outcome in terms of development speed and defect reduction."""
    return 'improved'

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

    return {
        "outcome": _outcome_of_refactoring_from(current_code_health),
    }