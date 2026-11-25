//! The median, the 95th percentile and the largest of a set of samples.

/// A set of samples, summarized.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

/// The sample at `rank`, from 0 to 1, by the nearest-rank method: the
/// smallest sample that at least that share of the samples does not
/// exceed. `sorted` is in ascending order and not empty.
fn nearest_rank(sorted: &[f64], rank: f64) -> f64 {
    let n = sorted.len();
    let at = ((rank * n as f64).ceil() as usize).clamp(1, n) - 1;
    sorted[at]
}

/// The summary of `samples`, or all zeros where there are none.
pub fn summarize(samples: &[f64]) -> Summary {
    if samples.is_empty() {
        return Summary::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    Summary {
        median: nearest_rank(&sorted, 0.5),
        p95: nearest_rank(&sorted, 0.95),
        max: sorted[sorted.len() - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_takes_the_nearest_rank() {
        let samples: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        let s = summarize(&samples);
        assert_eq!((s.median, s.p95, s.max), (10.0, 19.0, 20.0));
        assert_eq!(
            summarize(&[3.0]),
            Summary {
                median: 3.0,
                p95: 3.0,
                max: 3.0
            }
        );
        assert_eq!(summarize(&[]), Summary::default());
    }
}
