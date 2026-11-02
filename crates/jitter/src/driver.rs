//! The sampling loop. One thread, absolute deadlines, no drift.
//!
//! Every collector's deadlines sit on a grid `anchor + k * period`, and windows close on
//! `anchor + m * window`, where the anchor is a wall-clock second boundary. Deadlines are served
//! in order; at a tie the window closes first, so a sample belongs to the window its deadline
//! falls in, however late it is served. A loop that falls a whole period behind skips the
//! missed deadlines and counts them instead of bursting to catch up.

use crate::collectors::{CloseContext, CollectError, Collector};
use crate::self_report::SelfReport;
use crate::sink::{Batch, SinkHandle};
use jitter_core::line::Record;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Slot {
    collector: Box<dyn Collector>,
    period: Duration,
    k: u32,
}

impl Slot {
    fn deadline(&self, anchor: Instant) -> Instant {
        anchor + self.period * self.k
    }
}

/// Everything the loop owns.
pub struct Driver {
    slots: Vec<Slot>,
    self_report: Option<SelfReport>,
    window: Duration,
    service: String,
    host: String,
    stop: Arc<AtomicBool>,
}

impl Driver {
    /// Assemble the loop.
    pub fn new(
        collectors: Vec<Box<dyn Collector>>,
        self_report: Option<SelfReport>,
        window_seconds: u32,
        host: String,
        stop: Arc<AtomicBool>,
    ) -> Driver {
        Driver {
            slots: collectors
                .into_iter()
                .map(|c| Slot {
                    period: c.period(),
                    collector: c,
                    k: 0,
                })
                .collect(),
            self_report,
            window: Duration::from_secs(u64::from(window_seconds)),
            service: env!("CARGO_PKG_NAME").to_string(),
            host,
            stop,
        }
    }

    /// Run until `stop` is set or a collector fails. On stop, the partial window is closed and
    /// offered to the sink.
    pub fn run(mut self, sink: &mut SinkHandle) -> Result<(), CollectError> {
        let anchor = next_second_boundary();
        let mut m: u32 = 1;
        let mut records: Vec<Record> = Vec::new();
        let mut lines = String::new();
        loop {
            if self.stop.load(Ordering::Relaxed) {
                self.close_window(Instant::now(), &mut records, &mut lines, sink)?;
                return Ok(());
            }
            let next_close = anchor + self.window * m;
            let next_sample = self
                .slots
                .iter()
                .enumerate()
                .map(|(i, s)| (s.deadline(anchor), i))
                .min();
            let now = Instant::now();
            let due = next_sample.map_or(next_close, |(d, _)| d.min(next_close));
            if due > now {
                std::thread::sleep(due - now);
                continue;
            }
            match next_sample {
                Some((deadline, i)) if deadline < next_close => {
                    self.serve(i, deadline, anchor)?;
                }
                _ => {
                    self.close_window(now, &mut records, &mut lines, sink)?;
                    m = periods_elapsed(anchor, now, self.window) + 1;
                }
            }
        }
    }

    fn serve(&mut self, i: usize, deadline: Instant, anchor: Instant) -> Result<(), CollectError> {
        let slot = &mut self.slots[i];
        let t = Instant::now();
        slot.collector.sample(t)?;
        let late = t.saturating_duration_since(deadline);
        let next = periods_elapsed(anchor, t, slot.period) + 1;
        let skipped = next.saturating_sub(slot.k + 1);
        slot.k = next.max(slot.k + 1);
        if let Some(report) = &mut self.self_report {
            report.lateness(u64::try_from(late.as_nanos()).unwrap_or(u64::MAX))?;
            report.missed(u64::from(skipped));
        }
        Ok(())
    }

    fn close_window(
        &mut self,
        now: Instant,
        records: &mut Vec<Record>,
        lines: &mut String,
        sink: &mut SinkHandle,
    ) -> Result<(), CollectError> {
        let ctx = CloseContext {
            service: self.service.clone(),
            host: self.host.clone(),
            timestamp_ns: unix_ns(),
        };
        records.clear();
        for slot in &mut self.slots {
            slot.collector.close(&ctx, records);
        }
        if let Some(report) = &mut self.self_report {
            report.close(now, &ctx, records)?;
        }
        lines.clear();
        for r in records.iter() {
            r.write_to(lines);
        }
        if !lines.is_empty() {
            sink.offer(Batch {
                lines: std::mem::take(lines),
            });
        }
        Ok(())
    }
}

/// Whole periods between `anchor` and `t`; zero before the anchor.
fn periods_elapsed(anchor: Instant, t: Instant, period: Duration) -> u32 {
    let elapsed = t.saturating_duration_since(anchor).as_nanos();
    u32::try_from(elapsed / period.as_nanos().max(1)).unwrap_or(u32::MAX)
}

/// The monotonic instant of the next wall-clock second boundary, so windows from many hosts
/// close on the same seconds.
fn next_second_boundary() -> Instant {
    let now = Instant::now();
    let into_second = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    now + Duration::from_nanos(u64::from(1_000_000_000 - into_second))
}

fn unix_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

/// Upper bound on deadlines served per window, which sizes the lateness stream.
pub fn wakeups_per_window(collectors: &[Box<dyn Collector>], window_seconds: u32) -> usize {
    collectors
        .iter()
        .map(|c| {
            let per_second = 1_000_000_000 / c.period().as_nanos().max(1);
            usize::try_from(per_second).unwrap_or(usize::MAX) * window_seconds as usize
        })
        .sum()
}
