//! Rolling-window statistics: median, MAD, z-score-equivalent.
//!
//! All functions are pure and operate on sorted or unsorted `f64` slices.
//! No DB access here — these are building blocks for the detector algorithms.

/// Compute the median of a slice. Returns `None` for empty slices.
/// The input slice is cloned and sorted internally; the original is unchanged.
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 1 {
        Some(sorted[n / 2])
    } else {
        Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2.0)
    }
}

/// Median Absolute Deviation: median(|x_i - median(x)|).
/// Returns `None` for empty slices.
pub fn mad(values: &[f64]) -> Option<f64> {
    let m = median(values)?;
    let deviations: Vec<f64> = values.iter().map(|&x| (x - m).abs()).collect();
    median(&deviations)
}

/// Z-score equivalent using MAD: (value - median) / (MAD * 1.4826).
///
/// The 1.4826 constant makes MAD-based spread comparable to standard deviation
/// for normally-distributed data (consistency factor).
///
/// Returns `None` if the window is empty. Returns `f64::INFINITY` when MAD is
/// zero and the value differs from the median (spike on a perfectly flat
/// baseline), and `0.0` when both median and MAD are zero.
pub fn mad_zscore(value: f64, window: &[f64]) -> Option<f64> {
    let m = median(window)?;
    let d = mad(window)?;
    // Scaled MAD (consistency factor for Gaussian equivalence).
    let scaled = d * 1.482_6;
    if scaled == 0.0 {
        // Flat baseline: any deviation is infinitely anomalous.
        if (value - m).abs() < f64::EPSILON {
            Some(0.0)
        } else {
            Some(f64::INFINITY)
        }
    } else {
        Some((value - m) / scaled)
    }
}

/// Compute null rate: fraction of `total` that are null (null_count / total).
/// Returns 0.0 when total is zero.
pub fn null_rate(null_count: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        null_count as f64 / total as f64
    }
}

/// Shannon entropy normalised to [0, 1] over a discrete probability distribution.
///
/// `counts` gives the multiplicity of each category. Normalisation uses
/// log2(n_categories) so the result is 1.0 for a uniform distribution and 0.0
/// when one category dominates. Returns 0.0 for empty or single-category inputs.
pub fn normalized_entropy(counts: &[u64]) -> f64 {
    let non_zero: Vec<u64> = counts.iter().copied().filter(|&c| c > 0).collect();
    let n_categories = non_zero.len();
    if n_categories <= 1 {
        return 0.0;
    }
    let total: u64 = non_zero.iter().sum();
    let total_f = total as f64;
    let raw: f64 = non_zero
        .iter()
        .map(|&c| {
            let p = c as f64 / total_f;
            -p * p.log2()
        })
        .sum();
    // Normalise by max possible entropy (uniform distribution).
    let max_entropy = (n_categories as f64).log2();
    if max_entropy == 0.0 {
        0.0
    } else {
        raw / max_entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_empty() {
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn median_single() {
        assert_eq!(median(&[5.0]), Some(5.0));
    }

    #[test]
    fn median_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
    }

    #[test]
    fn median_even() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn mad_flat_is_zero() {
        let vals = vec![5.0; 10];
        assert_eq!(mad(&vals), Some(0.0));
    }

    #[test]
    fn mad_known_value() {
        // For [1,1,2,2,4,6,9], median=2, deviations=[1,1,0,0,2,4,7], median=1.
        let vals = [1.0, 1.0, 2.0, 2.0, 4.0, 6.0, 9.0];
        assert_eq!(mad(&vals), Some(1.0));
    }

    #[test]
    fn mad_zscore_flat_baseline_on_median() {
        let window = vec![100.0; 20];
        assert_eq!(mad_zscore(100.0, &window), Some(0.0));
    }

    #[test]
    fn mad_zscore_flat_baseline_spike() {
        let window = vec![100.0; 20];
        assert_eq!(mad_zscore(200.0, &window), Some(f64::INFINITY));
    }

    #[test]
    fn mad_zscore_normal_spread() {
        // A value exactly at the median should have z=0.
        let window = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let z = mad_zscore(5.5, &window).unwrap(); // median is 5.5
        assert!(z.abs() < 1e-9, "z at median should be ~0, got {z}");
    }

    #[test]
    fn mad_zscore_outlier_exceeds_threshold() {
        // Window of 100ms runs, spike to 5000ms.
        let window: Vec<f64> = (0..30).map(|_| 100.0).collect();
        let z = mad_zscore(5000.0, &window).unwrap();
        assert!(
            z > 5.0,
            "spike at 5000 on flat 100 window should exceed k=5, got {z}"
        );
    }

    #[test]
    fn null_rate_zero_total() {
        assert_eq!(null_rate(0, 0), 0.0);
    }

    #[test]
    fn null_rate_half() {
        assert!((null_rate(5, 10) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn entropy_uniform_is_one() {
        // Equal counts → max entropy → normalised = 1.0.
        let score = normalized_entropy(&[10, 10, 10, 10]);
        assert!(
            (score - 1.0).abs() < 1e-9,
            "uniform should be 1.0, got {score}"
        );
    }

    #[test]
    fn entropy_single_category_is_zero() {
        assert_eq!(normalized_entropy(&[100]), 0.0);
    }

    #[test]
    fn entropy_empty_is_zero() {
        assert_eq!(normalized_entropy(&[]), 0.0);
    }

    #[test]
    fn entropy_skewed_between_zero_and_one() {
        // One dominant polarity, one rare → entropy should be between 0 and 1.
        let score = normalized_entropy(&[90, 10]);
        assert!(score > 0.0 && score < 1.0, "skewed entropy={score}");
    }
}
