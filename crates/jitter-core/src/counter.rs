//! Monotonic kernel counters turned into rates.
//!
//! The kernel exports most activity as counters that only go up. A rate is the delta between two
//! reads divided by the measured elapsed time between them, never the nominal sample period: a
//! late wake-up stretches the interval, and dividing by the nominal period would report a spike
//! that never happened.

/// A counter that went backwards between two reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backwards {
    /// The earlier read.
    pub previous: u64,
    /// The later, smaller read.
    pub current: u64,
}

impl std::fmt::Display for Backwards {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "monotonic counter went backwards from {} to {}",
            self.previous, self.current
        )
    }
}

impl std::error::Error for Backwards {}

/// The increase from `previous` to `current`. A decrease is an error, not a wrap: every counter
/// Jitter reads is 64-bit and would take centuries to wrap, so a decrease means a reset (device
/// replaced, CPU hot-unplugged) and the stream's identity is no longer what it was at startup.
pub fn delta(previous: u64, current: u64) -> Result<u64, Backwards> {
    current
        .checked_sub(previous)
        .ok_or(Backwards { previous, current })
}

/// Holds the previous read of one counter so successive reads become deltas.
#[derive(Debug, Clone, Copy, Default)]
pub struct Last {
    previous: Option<u64>,
}

impl Last {
    /// Record `current` and return the delta from the previous read. The first read has no
    /// predecessor and returns `None`: there is no rate yet, and a zero would be a lie.
    pub fn advance(&mut self, current: u64) -> Result<Option<u64>, Backwards> {
        let out = match self.previous {
            Some(previous) => Some(delta(previous, current)?),
            None => None,
        };
        self.previous = Some(current);
        Ok(out)
    }
}
