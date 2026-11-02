//! `self`: Jitter's own footprint and timing, reported in the same stream as the host's.
//!
//! Wake-up lateness is how late each sampling deadline was served: kernel timer slack,
//! scheduling delay, and the loop's own work, together. CPU time and resident memory are the
//! footprint claim measured continuously rather than once.

use crate::collectors::{CloseContext, CollectError, Dist, Point, int};
use crate::sink::SinkStats;
use crate::source::Source;
use crate::sys;
use jitter_core::line::{FieldValue, Record};
use jitter_core::parse::scalar;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

/// The self report. Owned by the driver, which feeds it lateness directly.
pub struct SelfReport {
    lateness_us: Dist,
    cpu_percent: Point,
    rss_bytes: Point,
    missed: Point,
    dropped: Point,
    failed: Point,
    written: Point,
    statm: Source,
    page_size: u64,
    last_cpu: Option<(Instant, u64)>,
    missed_total: u64,
    sink: Arc<SinkStats>,
}

impl SelfReport {
    /// `wakeups_per_window` bounds the lateness stream.
    pub fn open(
        proc: &Path,
        wakeups_per_window: usize,
        sink: Arc<SinkStats>,
    ) -> Result<SelfReport, CollectError> {
        let mut statm = Source::open(proc.join("self/statm"))?;
        scalar::parse_statm_resident_pages(statm.read()?)?;
        Ok(SelfReport {
            lateness_us: Dist::new("jitter.self.wakeup_lateness_us", vec![], wakeups_per_window),
            cpu_percent: Point::new("jitter.self.cpu_percent", vec![]),
            rss_bytes: Point::new("jitter.self.resident_bytes", vec![]),
            missed: Point::new("jitter.self.missed_deadlines_total", vec![]),
            dropped: Point::new("jitter.sink.dropped_windows_total", vec![]),
            failed: Point::new("jitter.sink.failed_writes_total", vec![]),
            written: Point::new("jitter.sink.written_windows_total", vec![]),
            statm,
            page_size: sys::page_size(),
            last_cpu: None,
            missed_total: 0,
            sink,
        })
    }

    /// One served deadline.
    pub fn lateness(&mut self, late_ns: u64) -> Result<(), CollectError> {
        self.lateness_us.push(late_ns as f64 / 1000.0)
    }

    /// Deadlines skipped because the loop fell more than a period behind.
    pub fn missed(&mut self, count: u64) {
        self.missed_total += count;
    }

    /// Read footprint counters and reduce the window.
    pub fn close(
        &mut self,
        now: Instant,
        ctx: &CloseContext,
        out: &mut Vec<Record>,
    ) -> Result<(), CollectError> {
        let cpu_us = sys::self_cpu_us().map_err(|e| CollectError::Sys("getrusage", e))?;
        if let Some((at, previous)) = self.last_cpu {
            let wall_us = now.saturating_duration_since(at).as_secs_f64() * 1e6;
            if wall_us > 0.0 {
                let used = cpu_us.saturating_sub(previous) as f64;
                self.cpu_percent
                    .set(FieldValue::Float(100.0 * used / wall_us));
            }
        }
        self.last_cpu = Some((now, cpu_us));
        let pages = scalar::parse_statm_resident_pages(self.statm.read()?)?;
        self.rss_bytes.set(int(pages * self.page_size));
        self.missed.set(int(self.missed_total));
        self.dropped
            .set(int(self.sink.dropped.load(Ordering::Relaxed)));
        self.failed
            .set(int(self.sink.failed.load(Ordering::Relaxed)));
        self.written
            .set(int(self.sink.written.load(Ordering::Relaxed)));

        self.lateness_us.close(ctx, out);
        self.cpu_percent.close(ctx, out);
        self.rss_bytes.close(ctx, out);
        self.missed.close(ctx, out);
        self.dropped.close(ctx, out);
        self.failed.close(ctx, out);
        self.written.close(ctx, out);
        Ok(())
    }
}
