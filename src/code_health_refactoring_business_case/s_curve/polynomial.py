def polynomial(x: float, coeffs: list[float]) -> float:
    """Evaluate a polynomial at x using Horner's method."""
    y = 0.0
    for c in coeffs:
        y = y * x + c
    return y


def vectorized_polynomial(x: float, coeffs: list[list[float]]) -> list[float]:
    """Evaluate polynomial for each set of coefficients."""
    return [polynomial(x, c) for c in coeffs]
