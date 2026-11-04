//! Summary statistics and a significance test for per-iteration timings.

/// Summary of per-iteration timings, in nanoseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// Arithmetic mean.
    pub mean: f64,
    /// 50th percentile.
    pub median: f64,
    /// Fastest sample.
    pub min: f64,
    /// Slowest sample.
    pub max: f64,
    /// Sample standard deviation (n - 1); zero for a single sample.
    pub std_dev: f64,
    /// Median absolute deviation from the median, a spread measure robust to outliers.
    pub mad: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
}

impl Summary {
    /// Summarizes `samples`, or returns `None` when there are none.
    pub fn from_samples(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);

        let n = sorted.len() as f64;
        let mean = sorted.iter().sum::<f64>() / n;
        let std_dev = if sorted.len() > 1 {
            (sorted.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
        } else {
            0.0
        };
        let median = percentile(&sorted, 50.0);
        let mut deviations: Vec<f64> = sorted.iter().map(|x| (x - median).abs()).collect();
        deviations.sort_by(f64::total_cmp);

        Some(Self {
            mean,
            median,
            min: sorted[0],
            max: sorted[sorted.len() - 1],
            std_dev,
            mad: percentile(&deviations, 50.0),
            p95: percentile(&sorted, 95.0),
            p99: percentile(&sorted, 99.0),
        })
    }
}

/// The `p`th percentile (0 to 100) of `sorted`, interpolating linearly between ranks.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    let rank = p / 100.0 * (sorted.len() - 1) as f64;
    let (lower, upper) = (rank.floor() as usize, rank.ceil() as usize);
    sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64)
}

/// Two-sided p-value of the Mann-Whitney U test that `a` and `b` come from the same
/// distribution, using the normal approximation with tie and continuity corrections.
///
/// It compares ranks, not means, so a few slow outliers do not decide the answer. Returns
/// `None` when either side has fewer than two samples.
pub fn mann_whitney_p(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let (n1, n2) = (a.len() as f64, b.len() as f64);
    let total = n1 + n2;

    let mut pooled: Vec<(f64, bool)> = a
        .iter()
        .map(|&x| (x, true))
        .chain(b.iter().map(|&x| (x, false)))
        .collect();
    pooled.sort_by(|x, y| x.0.total_cmp(&y.0));

    let mut rank_sum_a = 0.0;
    let mut tie_term = 0.0;
    let mut start = 0;
    while start < pooled.len() {
        let mut end = start;
        while end < pooled.len() && pooled[end].0 == pooled[start].0 {
            end += 1;
        }
        // Tied values share the average of the ranks they span (ranks are 1-based).
        let average_rank = (start + 1 + end) as f64 / 2.0;
        let ties = (end - start) as f64;
        tie_term += ties.powi(3) - ties;
        rank_sum_a += pooled[start..end].iter().filter(|(_, from_a)| *from_a).count() as f64 * average_rank;
        start = end;
    }

    let u = rank_sum_a - n1 * (n1 + 1.0) / 2.0;
    let mean = n1 * n2 / 2.0;
    let variance = n1 * n2 / 12.0 * ((total + 1.0) - tie_term / (total * (total - 1.0)));
    if variance <= 0.0 {
        return Some(1.0);
    }
    let z = ((u - mean).abs() - 0.5).max(0.0) / variance.sqrt();
    Some(erfc(z / std::f64::consts::SQRT_2).clamp(0.0, 1.0))
}

/// Complementary error function for `x >= 0`, with fractional error below 1.2e-7.
fn erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.5 * x);
    t * (-x * x - 1.265_512_23
        + t * (1.000_023_68
            + t * (0.374_091_96
                + t * (0.096_784_18
                    + t * (-0.186_288_06
                        + t * (0.278_868_07
                            + t * (-1.135_203_98 + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
        .exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    #[test]
    fn empty_samples_have_no_summary() {
        assert_eq!(Summary::from_samples(&[]), None);
    }

    #[test]
    fn summary_of_a_known_series() {
        let summary = Summary::from_samples(&[5.0, 1.0, 3.0, 2.0, 4.0]).unwrap();
        assert_eq!(summary.mean, 3.0);
        assert_eq!(summary.median, 3.0);
        assert_eq!((summary.min, summary.max), (1.0, 5.0));
        assert!((summary.std_dev - 2.5_f64.sqrt()).abs() < 1e-12);
        assert_eq!(summary.mad, 1.0);
        assert!((summary.p95 - 4.8).abs() < 1e-12);
        assert!((summary.p99 - 4.96).abs() < 1e-12);
    }

    #[test]
    fn single_sample_has_no_spread() {
        let summary = Summary::from_samples(&[7.0]).unwrap();
        assert_eq!((summary.std_dev, summary.mad), (0.0, 0.0));
        assert_eq!((summary.median, summary.p99), (7.0, 7.0));
    }

    #[rstest]
    #[case::identical(&[10.0; 30], &[10.0; 30], 1.0, 1.0)]
    #[case::overlapping(&[10.0, 11.0, 12.0, 13.0, 14.0, 15.0], &[10.5, 11.5, 12.5, 13.5, 14.5, 15.5], 0.5, 1.0)]
    #[case::separated(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0], 0.0, 0.01)]
    fn mann_whitney_separates_distributions(#[case] a: &[f64], #[case] b: &[f64], #[case] low: f64, #[case] high: f64) {
        let p = mann_whitney_p(a, b).unwrap();
        assert!((low..=high).contains(&p), "p = {p}");
    }

    #[test]
    fn mann_whitney_needs_two_samples_per_side() {
        assert_eq!(mann_whitney_p(&[1.0], &[1.0, 2.0]), None);
        assert_eq!(mann_whitney_p(&[1.0, 2.0], &[]), None);
    }

    #[test]
    fn a_single_outlier_does_not_make_a_difference_significant() {
        let a: Vec<f64> = (0..30).map(|i| 100.0 + (i % 5) as f64).collect();
        let mut b = a.clone();
        b[0] = 10_000.0;
        assert!(mann_whitney_p(&a, &b).unwrap() > 0.5);
    }

    #[rstest]
    #[case(0.0, 1.0)]
    #[case(1.0, 0.157_299_2)]
    #[case(2.0, 0.004_677_735)]
    fn erfc_matches_reference_values(#[case] x: f64, #[case] expected: f64) {
        assert!((erfc(x) - expected).abs() <= expected * 1e-6, "erfc({x}) = {}", erfc(x));
    }

    proptest! {
        #[test]
        fn percentiles_are_ordered_and_bounded(samples in prop::collection::vec(0.0_f64..1e9, 1..200)) {
            let s = Summary::from_samples(&samples).unwrap();
            prop_assert!(s.min <= s.median && s.median <= s.p95 && s.p95 <= s.p99 && s.p99 <= s.max);
            prop_assert!(s.min <= s.mean + 1e-6 && s.mean <= s.max + 1e-6);
            prop_assert!(s.std_dev >= 0.0 && s.mad >= 0.0);
        }

        #[test]
        fn p_value_is_a_probability_and_symmetric(
            a in prop::collection::vec(0.0_f64..1e6, 2..60),
            b in prop::collection::vec(0.0_f64..1e6, 2..60),
        ) {
            let p = mann_whitney_p(&a, &b).unwrap();
            prop_assert!((0.0..=1.0).contains(&p));
            prop_assert!((p - mann_whitney_p(&b, &a).unwrap()).abs() < 1e-9);
        }
    }
}
