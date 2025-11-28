from typing import Any
import numpy as np
from pathlib import Path
from .regression import load_coefficients
from .polynomial import vectorized_polynomial

def _relative_change(baseline: Any, target: Any) -> Any:
    return 100 * (target - baseline) / baseline

def ci90(values: list[Any]) -> tuple[float, float]:
    return round(float(np.percentile(values, 5)), 2), round(float(np.percentile(values, 95)), 2)

def _path_to(coefficients_file):
    # Get the directory where this file is located, then navigate to regression/
    current_dir = Path(__file__).parent
    return f"{current_dir}/regression/{coefficients_file}"

def collect(code_health_baseline: float, code_health_target: float) -> dict[str, list[float]]:
    """Returns all measures for a pair of code health values and an initial unplanned work.

    Parameters
    code_health_baseline: Initial (basleine) code health.
    code_health_target: Final (target) code health.
    initial_unplanned_work_pct: Initial unplanned work in percentages.

    Returns
    -------
    Dictionary containing the defect reduction and time in development expressed 
    on CI90. (That gives us optimistic and pessimistic numbers).
    """
    # Defects
    defects_coeffs = load_coefficients(_path_to('defects.json'))
    defects_before = vectorized_polynomial(code_health_baseline, defects_coeffs)
    defects_after = vectorized_polynomial(code_health_target, defects_coeffs)
    
    # Time in development.
    time_coeffs = load_coefficients(_path_to('time.json'))
    time_before = vectorized_polynomial(code_health_baseline, time_coeffs)
    time_after = vectorized_polynomial(code_health_target, time_coeffs)

    return {
        'defects': ci90(_relative_change(defects_before, defects_after)),
        'time':    ci90(_relative_change(time_before, time_after))
    }
