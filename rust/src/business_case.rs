/// S-curve business case model.
///
/// Uses embedded polynomial regression coefficients to estimate the impact
/// of improving Code Health on defect rates and development time.

use serde::Serialize;

const DEFECTS_COEFFICIENTS: &str = include_str!("regression/defects.json");

const TIME_COEFFICIENTS: &str = include_str!("regression/time.json");

#[derive(Debug, Clone, Copy)]
struct HealthScore(f64);

impl HealthScore {
    fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
struct ScoreRange {
    baseline: HealthScore,
    target: HealthScore,
}

const SCENARIOS: &[(f64, &str)] = &[
    (5.15, "industry average"),
    (9.1, "top 5%"),
    (10.0, "optimal"),
];

#[derive(Debug, Clone, Serialize)]
pub struct BusinessCase {
    pub scenario: String,
    pub target_score: f64,
    pub current_score: f64,
    pub optimistic_outcome: Outcome,
    pub pessimistic_outcome: Outcome,
    pub confidence_interval: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub defect_reduction_percent: f64,
    pub time_reduction_percent: f64,
}

pub fn make_business_case(current_score: f64) -> Option<BusinessCase> {
    let current = HealthScore(current_score);
    let (target_score, label) = find_target_scenario(current)?;
    let range = ScoreRange { baseline: current, target: target_score };
    let metrics = collect_metrics(range);

    let (defect_pessimistic, defect_optimistic) = metrics.defects;
    let (time_pessimistic, time_optimistic) = metrics.time;

    Some(BusinessCase {
        scenario: label.to_string(),
        target_score: target_score.value(),
        current_score,
        optimistic_outcome: Outcome {
            defect_reduction_percent: defect_optimistic,
            time_reduction_percent: time_optimistic,
        },
        pessimistic_outcome: Outcome {
            defect_reduction_percent: defect_pessimistic,
            time_reduction_percent: time_pessimistic,
        },
        confidence_interval: format!(
            "90% CI: defects [{defect_pessimistic:.1}%, {defect_optimistic:.1}%], \
             time [{time_pessimistic:.1}%, {time_optimistic:.1}%]"
        ),
    })
}

fn find_target_scenario(current: HealthScore) -> Option<(HealthScore, &'static str)> {
    SCENARIOS
        .iter()
        .find(|(target, _)| *target > current.value())
        .map(|(t, l)| (HealthScore(*t), *l))
}

struct Metrics {
    defects: (f64, f64),
    time: (f64, f64),
}

fn collect_metrics(range: ScoreRange) -> Metrics {
    let defect_coeffs = load_coefficients(DEFECTS_COEFFICIENTS);
    let time_coeffs = load_coefficients(TIME_COEFFICIENTS);

    let defects_before = vectorized_polynomial(range.baseline, &defect_coeffs);
    let defects_after = vectorized_polynomial(range.target, &defect_coeffs);
    let defect_changes = relative_change(&defects_before, &defects_after);

    let time_before = vectorized_polynomial(range.baseline, &time_coeffs);
    let time_after = vectorized_polynomial(range.target, &time_coeffs);
    let time_changes = relative_change(&time_before, &time_after);

    Metrics {
        defects: ci90(&defect_changes),
        time: ci90(&time_changes),
    }
}

fn load_coefficients(ndjson: &str) -> Vec<Vec<f64>> {
    ndjson
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            let coeffs = v.get("coeffs")?.as_array()?;
            Some(
                coeffs
                    .iter()
                    .filter_map(|c| c.as_f64())
                    .collect::<Vec<f64>>(),
            )
        })
        .collect()
}

/// Evaluate a polynomial at `x` using Horner's method.
fn polynomial(x: f64, coeffs: &[f64]) -> f64 {
    coeffs.iter().fold(0.0, |acc, &c| acc * x + c)
}

fn vectorized_polynomial(score: HealthScore, all_coeffs: &[Vec<f64>]) -> Vec<f64> {
    all_coeffs.iter().map(|c| polynomial(score.value(), c)).collect()
}

fn relative_change(baseline: &[f64], target: &[f64]) -> Vec<f64> {
    baseline
        .iter()
        .zip(target.iter())
        .map(|(b, t)| 100.0 * (t - b) / b)
        .collect()
}

fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }

    let rank = (p / 100.0) * (n as f64 - 1.0);
    let lower = rank as usize;
    let upper = (lower + 1).min(n - 1);
    let fraction = rank - lower as f64;

    values[lower] + fraction * (values[upper] - values[lower])
}

fn ci90(values: &[f64]) -> (f64, f64) {
    let mut sorted = values.to_vec();
    let p5 = round2(percentile(&mut sorted.clone(), 5.0));
    let p95 = round2(percentile(&mut sorted, 95.0));
    (p5, p95)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_horner_polynomial() {
        // 2x^2 + 3x + 1 at x=2 => 8 + 6 + 1 = 15
        assert!((polynomial(2.0, &[2.0, 3.0, 1.0]) - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_business_case_baseline_2() {
        let range = ScoreRange {
            baseline: HealthScore(2.0),
            target: HealthScore(5.15),
        };
        let metrics = collect_metrics(range);
        assert_eq!(metrics.defects, (-65.6, -39.37));
        assert_eq!(metrics.time, (-29.18, -5.58));
    }

    #[test]
    fn test_business_case_baseline_2_target_10() {
        let range = ScoreRange {
            baseline: HealthScore(2.0),
            target: HealthScore(10.0),
        };
        let metrics = collect_metrics(range);
        assert_eq!(metrics.defects, (-91.24, -84.78));
        assert_eq!(metrics.time, (-66.04, -52.3));
    }

    #[test]
    fn test_find_scenario_for_low_score() {
        let (target, label) = find_target_scenario(HealthScore(3.0)).unwrap();
        assert_eq!(target.value(), 5.15);
        assert_eq!(label, "industry average");
    }

    #[test]
    fn test_find_scenario_for_high_score() {
        let (target, label) = find_target_scenario(HealthScore(9.5)).unwrap();
        assert_eq!(target.value(), 10.0);
        assert_eq!(label, "optimal");
    }

    #[test]
    fn test_no_scenario_at_perfect() {
        assert!(find_target_scenario(HealthScore(10.0)).is_none());
    }

    // ---- make_business_case ----

    #[test]
    fn make_business_case_low_score() {
        let bc = make_business_case(2.0).unwrap();
        assert_eq!(bc.scenario, "industry average");
        assert_eq!(bc.target_score, 5.15);
        assert_eq!(bc.current_score, 2.0);
        assert!(bc.optimistic_outcome.defect_reduction_percent < 0.0);
        assert!(bc.pessimistic_outcome.defect_reduction_percent < 0.0);
        assert!(bc.confidence_interval.contains("CI"));
    }

    #[test]
    fn make_business_case_medium_score() {
        let bc = make_business_case(6.0).unwrap();
        assert_eq!(bc.scenario, "top 5%");
        assert_eq!(bc.target_score, 9.1);
    }

    #[test]
    fn make_business_case_high_score() {
        let bc = make_business_case(9.5).unwrap();
        assert_eq!(bc.scenario, "optimal");
        assert_eq!(bc.target_score, 10.0);
    }

    #[test]
    fn make_business_case_perfect_returns_none() {
        assert!(make_business_case(10.0).is_none());
    }

    #[test]
    fn make_business_case_above_all_scenarios_returns_none() {
        // 10.0 is the highest scenario target, so anything >= 10.0 returns None
        assert!(make_business_case(10.5).is_none());
    }

    // ---- percentile edge cases ----

    #[test]
    fn percentile_empty_returns_zero() {
        assert_eq!(percentile(&mut [], 50.0), 0.0);
    }

    #[test]
    fn percentile_single_value() {
        assert_eq!(percentile(&mut [42.0], 50.0), 42.0);
    }

    #[test]
    fn percentile_two_values() {
        let p50 = percentile(&mut [10.0, 20.0], 50.0);
        assert!((p50 - 15.0).abs() < 1e-10);
    }

    // ---- round2 ----

    #[test]
    fn round2_works() {
        assert_eq!(round2(1.234), 1.23);
        assert_eq!(round2(1.235), 1.24);
        assert_eq!(round2(0.0), 0.0);
    }

    // ---- relative_change ----

    #[test]
    fn relative_change_simple() {
        let baseline = vec![100.0, 200.0];
        let target = vec![80.0, 250.0];
        let changes = relative_change(&baseline, &target);
        assert!((changes[0] - (-20.0)).abs() < 1e-10);
        assert!((changes[1] - 25.0).abs() < 1e-10);
    }

    // ---- load_coefficients ----

    #[test]
    fn load_coefficients_parses_ndjson() {
        let ndjson = r#"{"coeffs": [1.0, 2.0, 3.0]}
{"coeffs": [4.0, 5.0]}
"#;
        let coeffs = load_coefficients(ndjson);
        assert_eq!(coeffs.len(), 2);
        assert_eq!(coeffs[0], vec![1.0, 2.0, 3.0]);
        assert_eq!(coeffs[1], vec![4.0, 5.0]);
    }

    #[test]
    fn load_coefficients_skips_empty_lines() {
        let ndjson = "\n{\"coeffs\": [1.0]}\n\n";
        let coeffs = load_coefficients(ndjson);
        assert_eq!(coeffs.len(), 1);
    }

    #[test]
    fn load_coefficients_skips_invalid_lines() {
        let ndjson = "not json\n{\"coeffs\": [1.0]}\n{\"no_coeffs\": true}\n";
        let coeffs = load_coefficients(ndjson);
        assert_eq!(coeffs.len(), 1);
    }

    // ---- BusinessCase serialisation ----

    #[test]
    fn business_case_serializes() {
        let bc = make_business_case(2.0).unwrap();
        let json = serde_json::to_string(&bc).unwrap();
        assert!(json.contains("\"scenario\""));
        assert!(json.contains("\"target_score\""));
        assert!(json.contains("\"optimistic_outcome\""));
        assert!(json.contains("\"pessimistic_outcome\""));
        assert!(json.contains("\"confidence_interval\""));
    }
}
