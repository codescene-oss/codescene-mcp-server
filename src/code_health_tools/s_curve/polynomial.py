import numpy as np
from typing import Any


def polynomial(x: Any, coeffs: Any) -> float:
    y = 0
    for c in coeffs:
        y = y * x + c
    return y


def is_monotonous(coeffs: list[float]) -> bool:
    return coeffs[1]**2 - 3 * coeffs[0] * coeffs[2] < 0


def vectorized_polynomial(x: Any, coeffs: Any) -> Any:
    return np.apply_along_axis(lambda c: polynomial(x, c), 1, coeffs)
