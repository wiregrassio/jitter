//! `timesync`: the kernel's clock discipline state, read with `adjtimex`.
//!
//! Every NTP and PTP client steers the clock through the kernel, so reading the kernel works
//! whichever client is installed and needs no client socket. The error bound replaces the
//! client-only notion of stratum: it grows without bound once synchronization is lost.

use super::{CloseContext, CollectError, Collector, Dist, Point, capacity, period};
use crate::sys;
use jitter_core::line::{FieldValue, Record};
use std::time::{Duration, Instant};

/// The timesync collector.
pub struct Timesync {
    period: Duration,
    offset: Dist,
    frequency: Point,
    max_error: Point,
    synchronized: Point,
}

impl Timesync {
    /// Check `adjtimex` answers.
    pub fn open(hz: u32, window_seconds: u32) -> Result<Timesync, CollectError> {
        sys::clock_state().map_err(|e| CollectError::Sys("adjtimex", e))?;
        Ok(Timesync {
            period: period(hz),
            offset: Dist::new(
                "system.clock.offset_seconds",
                vec![],
                capacity(hz, window_seconds),
            ),
            frequency: Point::new("system.clock.frequency_ppm", vec![]),
            max_error: Point::new("system.clock.max_error_seconds", vec![]),
            synchronized: Point::new("system.clock.synchronized", vec![]),
        })
    }
}

impl Collector for Timesync {
    fn name(&self) -> &'static str {
        "timesync"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, _now: Instant) -> Result<(), CollectError> {
        let s = sys::clock_state().map_err(|e| CollectError::Sys("adjtimex", e))?;
        self.offset.push(s.offset_seconds)?;
        self.frequency.set(FieldValue::Float(s.frequency_ppm));
        self.max_error.set(FieldValue::Float(s.max_error_seconds));
        self.synchronized
            .set(FieldValue::Int(i64::from(s.synchronized)));
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        self.offset.close(ctx, out);
        self.frequency.close(ctx, out);
        self.max_error.close(ctx, out);
        self.synchronized.close(ctx, out);
    }
}
