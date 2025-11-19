import numpy as np
import polars as pl
from tqdm import tqdm
from typing import Any
from .polynomial import is_monotonous


def bootstrapped_regression(data: pl.DataFrame, x_column: str, y_column: str, num_samples: int) -> list[list[float]]:
    """Runs a bootstrapped regression on a dataset to obtain a set of sampled coefficients.
    Used to sample coefficients for defects and time in development.
    For more details, see: https://arxiv.org/pdf/2401.13407.

    Parameters
    ----------
    data: Polars dataframe containing the observeations.
    x_column: X column of the regression.
    y_column: Y column of the regression.
    num_samples: Number of sampled coefficients.

    Returns
    -------
    List of lists containing the coefficients.
    """
    prepared_data = data.select(x_column, y_column).drop_nulls()
    sampled_coeffs = []
    samples_left = 0
    pbar = tqdm(total=num_samples, desc='sampling coefficients')
    while samples_left < num_samples:
        # Do regression on sub-sampled data.
        sampled_data = prepared_data.sample(n=10_000, with_replacement=True)
        coeffs = np.polyfit(sampled_data[x_column], sampled_data[y_column], deg=3).tolist()
        if not is_monotonous(coeffs):
            continue

        # Add plot otherwise.
        sampled_coeffs.append(coeffs)
        samples_left += 1
        pbar.update()

    return sampled_coeffs


def load_coefficients(path: str) -> list[Any]:
    return pl.read_ndjson(path)['coeffs'].to_list()
