//! `memory`: available memory, swap, and the major page fault rate.

use super::{
    CloseContext, CollectError, Collector, Dist, Point, capacity, elapsed_seconds, int, period,
};
use crate::source::Source;
use jitter_core::counter::Last;
use jitter_core::line::Record;
use jitter_core::parse::meminfo;
use std::path::Path;
use std::time::{Duration, Instant};

/// The memory collector.
pub struct Memory {
    meminfo: Source,
    vmstat: Source,
    period: Duration,
    available: Dist,
    total: Point,
    swap_total: Point,
    swap_free: Point,
    faults: Last,
    last_at: Option<Instant>,
    major_faults: Dist,
}

impl Memory {
    /// Open `<proc>/meminfo` and `<proc>/vmstat` and check both parse.
    pub fn open(proc: &Path, hz: u32, window_seconds: u32) -> Result<Memory, CollectError> {
        let mut meminfo_src = Source::open(proc.join("meminfo"))?;
        let mut vmstat_src = Source::open(proc.join("vmstat"))?;
        meminfo::parse_meminfo(meminfo_src.read()?)?;
        meminfo::parse_vmstat(vmstat_src.read()?, "pgmajfault")?;
        let cap = capacity(hz, window_seconds);
        Ok(Memory {
            meminfo: meminfo_src,
            vmstat: vmstat_src,
            period: period(hz),
            available: Dist::new("system.memory.available_bytes", vec![], cap),
            total: Point::new("system.memory.total_bytes", vec![]),
            swap_total: Point::new("system.memory.swap_total_bytes", vec![]),
            swap_free: Point::new("system.memory.swap_free_bytes", vec![]),
            faults: Last::default(),
            last_at: None,
            major_faults: Dist::new("system.memory.major_faults_per_second", vec![], cap),
        })
    }
}

impl Collector for Memory {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, now: Instant) -> Result<(), CollectError> {
        let m = meminfo::parse_meminfo(self.meminfo.read()?)?;
        self.available.push(m.available_bytes as f64)?;
        self.total.set(int(m.total_bytes));
        self.swap_total.set(int(m.swap_total_bytes));
        self.swap_free.set(int(m.swap_free_bytes));

        let faults = meminfo::parse_vmstat(self.vmstat.read()?, "pgmajfault")?;
        let delta = self
            .faults
            .advance(faults)
            .map_err(|e| CollectError::Counter("pgmajfault", e))?;
        if let (Some(delta), Some(at)) = (delta, self.last_at) {
            let secs = elapsed_seconds(at, now);
            if secs > 0.0 {
                self.major_faults.push(delta as f64 / secs)?;
            }
        }
        self.last_at = Some(now);
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        self.available.close(ctx, out);
        self.total.close(ctx, out);
        self.swap_total.close(ctx, out);
        self.swap_free.close(ctx, out);
        self.major_faults.close(ctx, out);
    }
}
