//! `sched`: per-CPU run time and run-queue wait, from `/proc/schedstat` nanoseconds.
//!
//! These counters advance when a task switches out, so a CPU running one task without a switch
//! reads 0 % until that task yields. Run-queue wait is the useful signal: time runnable tasks
//! spent waiting for a CPU, which is scheduling noise measured directly.

use super::{CloseContext, CollectError, Collector, Dist, capacity, elapsed_seconds, period};
use crate::source::Source;
use jitter_core::counter::Last;
use jitter_core::line::Record;
use jitter_core::parse::schedstat::{self, CpuSched};
use std::path::Path;
use std::time::{Duration, Instant};

struct PerCpu {
    id: u32,
    running: Last,
    waiting: Last,
    running_percent: Dist,
    wait_percent: Dist,
}

/// The sched collector.
pub struct Sched {
    source: Source,
    period: Duration,
    cpus: Vec<PerCpu>,
    last_at: Option<Instant>,
    scratch: Vec<CpuSched>,
}

impl Sched {
    /// Open `<proc>/schedstat`, check its version, and fix the CPU set seen at startup.
    pub fn open(proc: &Path, hz: u32, window_seconds: u32) -> Result<Sched, CollectError> {
        let mut source = Source::open(proc.join("schedstat"))?;
        let mut scratch = Vec::new();
        schedstat::parse(source.read()?, &mut scratch)?;
        let cap = capacity(hz, window_seconds);
        let cpus = scratch
            .iter()
            .map(|s| PerCpu {
                id: s.cpu,
                running: Last::default(),
                waiting: Last::default(),
                running_percent: Dist::new(
                    "system.sched.running_percent",
                    vec![("cpu", s.cpu.to_string())],
                    cap,
                ),
                wait_percent: Dist::new(
                    "system.sched.runqueue_wait_percent",
                    vec![("cpu", s.cpu.to_string())],
                    cap,
                ),
            })
            .collect();
        Ok(Sched {
            source,
            period: period(hz),
            cpus,
            last_at: None,
            scratch,
        })
    }
}

impl Collector for Sched {
    fn name(&self) -> &'static str {
        "sched"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, now: Instant) -> Result<(), CollectError> {
        schedstat::parse(self.source.read()?, &mut self.scratch)?;
        if self.scratch.len() != self.cpus.len()
            || self
                .scratch
                .iter()
                .zip(&self.cpus)
                .any(|(s, c)| s.cpu != c.id)
        {
            return Err(CollectError::Missing(
                "the CPU set seen at startup: a CPU went offline or came online".into(),
            ));
        }
        let elapsed_ns = self.last_at.map(|t| elapsed_seconds(t, now) * 1e9);
        self.last_at = Some(now);
        for (s, cpu) in self.scratch.iter().zip(&mut self.cpus) {
            let running = cpu
                .running
                .advance(s.running_ns)
                .map_err(|e| CollectError::Counter("schedstat running ns", e))?;
            let waiting = cpu
                .waiting
                .advance(s.waiting_ns)
                .map_err(|e| CollectError::Counter("schedstat waiting ns", e))?;
            if let (Some(running), Some(waiting), Some(ns)) = (running, waiting, elapsed_ns)
                && ns > 0.0
            {
                cpu.running_percent.push(100.0 * running as f64 / ns)?;
                cpu.wait_percent.push(100.0 * waiting as f64 / ns)?;
            }
        }
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        for cpu in &mut self.cpus {
            cpu.running_percent.close(ctx, out);
            cpu.wait_percent.close(ctx, out);
        }
    }
}
