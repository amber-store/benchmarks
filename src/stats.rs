//! Aggregation of repeated samples.
//!
//! Benchmarks here report the raw samples *and* a summary; the summary never
//! replaces the samples in the machine-readable output. Every method used is
//! named in the output so a reader never has to guess which "p95" this is.

use serde::Serialize;

/// How the summary of a set of samples was computed.
pub const METHOD: &str = "median = lower of the two middle samples for even n; p95 = nearest-rank \
     (sorted ascending, index ceil(0.95*n)-1); stddev = sample standard \
     deviation with n-1 denominator (null for n < 2); cv = stddev/mean";

/// Summary of one metric over the repetitions of one operation.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    /// Number of samples that contributed.
    pub n: usize,
    /// Unit of every value below, e.g. `ns` or `bytes`.
    pub unit: String,
    pub min: f64,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
    pub mean: f64,
    /// Sample standard deviation, `null` for a single sample.
    pub stddev: Option<f64>,
    /// Coefficient of variation (stddev/mean), `null` when undefined.
    pub cv: Option<f64>,
    /// The exact definitions used, repeated in every summary so a fragment of
    /// the report is still self-describing.
    pub method: String,
}

/// Summarises `samples`, or returns `None` when there are none. Never invents
/// a value: an operation with no successful sample has no summary at all.
pub fn summarize(samples: &[f64], unit: &str) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    let mean = s.iter().sum::<f64>() / n as f64;
    let stddev = if n > 1 {
        let var = s.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        Some(var.sqrt())
    } else {
        None
    };
    Some(Summary {
        n,
        unit: unit.to_string(),
        min: s[0],
        median: median(&s),
        p95: nearest_rank(&s, 0.95),
        max: s[n - 1],
        mean,
        stddev,
        cv: stddev.and_then(|sd| if mean != 0.0 { Some(sd / mean) } else { None }),
        method: METHOD.to_string(),
    })
}

/// Median of an ascending slice. For an even count this is the lower of the
/// two middle samples rather than their mean, so the reported median is
/// always a value that was actually observed.
fn median(sorted: &[f64]) -> f64 {
    sorted[(sorted.len() - 1) / 2]
}

/// Nearest-rank percentile of an ascending slice.
fn nearest_rank(sorted: &[f64], q: f64) -> f64 {
    let rank = (q * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_samples_yields_no_summary() {
        assert!(summarize(&[], "ns").is_none());
    }

    #[test]
    fn single_sample_has_no_dispersion() {
        let s = summarize(&[5.0], "ns").unwrap();
        assert_eq!(s.n, 1);
        assert_eq!(s.median, 5.0);
        assert_eq!(s.p95, 5.0);
        assert_eq!(s.min, 5.0);
        assert_eq!(s.max, 5.0);
        assert!(s.stddev.is_none(), "one sample cannot have a stddev");
        assert!(s.cv.is_none());
    }

    #[test]
    fn median_is_an_observed_sample() {
        assert_eq!(summarize(&[1.0, 2.0, 3.0, 4.0], "ns").unwrap().median, 2.0);
        assert_eq!(summarize(&[3.0, 1.0, 2.0], "ns").unwrap().median, 2.0);
    }

    #[test]
    fn p95_uses_nearest_rank() {
        let samples: Vec<f64> = (1..=100).map(|v| v as f64).collect();
        assert_eq!(summarize(&samples, "ns").unwrap().p95, 95.0);
        // With 3 samples, ceil(0.95*3) = 3, i.e. the maximum.
        assert_eq!(summarize(&[1.0, 2.0, 9.0], "ns").unwrap().p95, 9.0);
    }

    #[test]
    fn stddev_uses_the_sample_denominator() {
        let s = summarize(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], "ns").unwrap();
        assert_eq!(s.mean, 5.0);
        // Population sd is 2.0; sample sd is sqrt(32/7).
        let want = (32.0f64 / 7.0).sqrt();
        assert!((s.stddev.unwrap() - want).abs() < 1e-12, "{s:?}");
        assert!((s.cv.unwrap() - want / 5.0).abs() < 1e-12);
    }

    #[test]
    fn summary_carries_its_unit_and_method() {
        let s = summarize(&[1.0], "bytes").unwrap();
        assert_eq!(s.unit, "bytes");
        assert!(s.method.contains("nearest-rank"));
    }
}
