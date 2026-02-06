import json
from pathlib import Path

from .polynomial import vectorized_polynomial


def _relative_change(baseline: list[float], target: list[float]) -> list[float]:
    """Calculate relative change as percentage."""
    return [100 * (t - b) / b for b, t in zip(baseline, target, strict=False)]


def percentile(values: list[float], p: float) -> float:
    """Calculate the p-th percentile of a list of values.

    Uses linear interpolation between closest ranks (same as numpy's default).
    """
    if not values:
        raise ValueError("Cannot compute percentile of empty list")

    sorted_values = sorted(values)
    n = len(sorted_values)

    # Calculate the rank (0-indexed position)
    rank = (p / 100) * (n - 1)
    lower_idx = int(rank)
    upper_idx = min(lower_idx + 1, n - 1)
    fraction = rank - lower_idx

    # Linear interpolation
    return sorted_values[lower_idx] + fraction * (sorted_values[upper_idx] - sorted_values[lower_idx])


def ci90(values: list[float]) -> tuple[float, float]:
    """Calculate 90% confidence interval (5th and 95th percentiles)."""
    return round(percentile(values, 5), 2), round(percentile(values, 95), 2)


def _path_to(coefficients_file: str) -> str:
    # Get the directory where this file is located, then navigate to regression/
    current_dir = Path(__file__).parent
    return f"{current_dir}/regression/{coefficients_file}"


def load_coefficients(path: str) -> list[list[float]]:
    """Load coefficients from an NDJSON file (newline-delimited JSON)."""
    coefficients = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                data = json.loads(line)
                coefficients.append(data["coeffs"])
    return coefficients


def collect(code_health_baseline: float, code_health_target: float) -> dict[str, tuple[float, float]]:
    """Returns all measures for a pair of code health values and an initial unplanned work.

    Parameters
    ----------
    code_health_baseline: Initial (baseline) code health.
    code_health_target: Final (target) code health.

    Returns
    -------
    Dictionary containing the defect reduction and time in development expressed
    on CI90. (That gives us optimistic and pessimistic numbers).
    """
    # Defects
    defects_coeffs = load_coefficients(_path_to("defects.json"))
    defects_before = vectorized_polynomial(code_health_baseline, defects_coeffs)
    defects_after = vectorized_polynomial(code_health_target, defects_coeffs)

    # Time in development.
    time_coeffs = load_coefficients(_path_to("time.json"))
    time_before = vectorized_polynomial(code_health_baseline, time_coeffs)
    time_after = vectorized_polynomial(code_health_target, time_coeffs)

    return {
        "defects": ci90(_relative_change(defects_before, defects_after)),
        "time": ci90(_relative_change(time_before, time_after)),
    }
