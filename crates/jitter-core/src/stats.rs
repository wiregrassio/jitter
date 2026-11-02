//! Window reduction: a bounded buffer of samples in, one fixed summary out.
//!
//! Only statistics that recombine across windows are kept. `count`, `min`, `max`, and `mean`
//! recombine exactly (count-weighted for the mean); the percentiles recombine approximately, and
//! a reader of long-range percentiles is reading an estimate. Higher moments are not computed.

/// The fixed summary of one window. Field order here is the field order on the wire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    /// Samples in the window; at least 1.
    pub count: u64,
    /// Smallest sample.
    pub min: f64,
    /// 5th percentile, nearest rank.
    pub p5: f64,
    /// Median, nearest rank.
    pub p50: f64,
    /// 95th percentile, nearest rank.
    pub p95: f64,
    /// Largest sample.
    pub max: f64,
    /// Arithmetic mean.
    pub mean: f64,
}

/// Why a sample was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowError {
    /// The window already holds its configured capacity. The driver sized the window from the
    /// validated sample rate, so this is a scheduling bug, not load: crash on it.
    Full {
        /// The configured capacity.
        capacity: usize,
    },
    /// NaN or infinity. A parser or rate computation produced garbage.
    NotFinite,
}

impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowError::Full { capacity } => write!(f, "window full at {capacity} samples"),
            WindowError::NotFinite => write!(f, "sample is not a finite number"),
        }
    }
}

impl std::error::Error for WindowError {}

/// A preallocated sample buffer for one stream. It never grows after construction, so a window
/// costs the same memory at hour one and at day ninety.
#[derive(Debug, Clone)]
pub struct Window {
    samples: Vec<f64>,
    capacity: usize,
}

impl Window {
    /// Allocate a window that holds at most `capacity` samples.
    pub fn with_capacity(capacity: usize) -> Self {
        Window {
            samples: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Add one sample.
    pub fn push(&mut self, sample: f64) -> Result<(), WindowError> {
        if !sample.is_finite() {
            return Err(WindowError::NotFinite);
        }
        if self.samples.len() >= self.capacity {
            return Err(WindowError::Full {
                capacity: self.capacity,
            });
        }
        self.samples.push(sample);
        Ok(())
    }

    /// Samples currently held.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when no sample has been pushed since the last close.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Reduce the window and empty it. An empty window yields `None`: no samples means no record,
    /// never a record of zeros.
    pub fn close(&mut self) -> Option<Summary> {
        let summary = summarize(&mut self.samples);
        self.samples.clear();
        summary
    }
}

/// Reduce samples to a [`Summary`]. Sorts `samples` in place.
pub fn summarize(samples: &mut [f64]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable_by(f64::total_cmp);
    let n = samples.len();
    let sum: f64 = samples.iter().sum();
    Some(Summary {
        count: n as u64,
        min: samples[0],
        p5: nearest_rank(samples, 5),
        p50: nearest_rank(samples, 50),
        p95: nearest_rank(samples, 95),
        max: samples[n - 1],
        mean: sum / n as f64,
    })
}

/// Nearest-rank percentile of sorted, non-empty `sorted`: the smallest sample with at least
/// `pct` percent of the samples at or below it. Integer arithmetic, so no rounding drift.
fn nearest_rank(sorted: &[f64], pct: usize) -> f64 {
    let n = sorted.len();
    let rank = (pct * n).div_ceil(100).max(1);
    sorted[rank - 1]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_matches_definition_for_one_to_one_hundred_samples() {
        for n in 1..=100usize {
            let mut v: Vec<f64> = (1..=n).map(|i| i as f64).collect();
            let s = summarize(&mut v).unwrap();
            for (pct, got) in [(5, s.p5), (50, s.p50), (95, s.p95)] {
                let at_or_below = |x: f64| v.iter().filter(|&&y| y <= x).count();
                assert!(at_or_below(got) * 100 >= pct * n, "n={n} p{pct}={got}");
                assert!(
                    got == 1.0 || at_or_below(got - 1.0) * 100 < pct * n,
                    "n={n} p{pct}={got} not the smallest"
                );
            }
        }
    }
}
