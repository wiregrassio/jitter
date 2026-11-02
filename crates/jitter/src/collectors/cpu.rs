//! `cpu`: utilization and steal per CPU and in aggregate, from `/proc/stat` ticks.
//!
//! Utilization is busy ticks over all ticks between two reads, so it needs no clock. Its
//! resolution is one tick over the ticks a sample spans, which is why config caps the rate.

use super::{CloseContext, CollectError, Collector, Dist, capacity, period};
use crate::source::Source;
use jitter_core::counter::Last;
use jitter_core::line::Record;
use jitter_core::parse::proc_stat::{self, CpuTicks};
use std::path::Path;
use std::time::{Duration, Instant};

struct PerCpu {
    id: Option<u32>,
    busy: Last,
    total: Last,
    steal: Last,
    utilization: Dist,
    steal_percent: Dist,
}

/// The cpu collector.
pub struct Cpu {
    source: Source,
    period: Duration,
    cpus: Vec<PerCpu>,
    scratch: Vec<CpuTicks>,
}

impl Cpu {
    /// Open `<proc>/stat` and fix the CPU set seen at startup.
    pub fn open(proc: &Path, hz: u32, window_seconds: u32) -> Result<Cpu, CollectError> {
        let mut source = Source::open(proc.join("stat"))?;
        let mut scratch = Vec::new();
        proc_stat::parse(source.read()?, &mut scratch)?;
        let cap = capacity(hz, window_seconds);
        let cpus = scratch
            .iter()
            .map(|t| {
                let label = t.cpu.map_or_else(|| "all".to_string(), |n| n.to_string());
                PerCpu {
                    id: t.cpu,
                    busy: Last::default(),
                    total: Last::default(),
                    steal: Last::default(),
                    utilization: Dist::new(
                        "system.cpu.utilization_percent",
                        vec![("cpu", label.clone())],
                        cap,
                    ),
                    steal_percent: Dist::new("system.cpu.steal_percent", vec![("cpu", label)], cap),
                }
            })
            .collect();
        Ok(Cpu {
            source,
            period: period(hz),
            cpus,
            scratch,
        })
    }
}

impl Collector for Cpu {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, _now: Instant) -> Result<(), CollectError> {
        proc_stat::parse(self.source.read()?, &mut self.scratch)?;
        if self.scratch.len() != self.cpus.len()
            || self
                .scratch
                .iter()
                .zip(&self.cpus)
                .any(|(t, c)| t.cpu != c.id)
        {
            return Err(CollectError::Missing(
                "the CPU set seen at startup: a CPU went offline or came online".into(),
            ));
        }
        for (ticks, cpu) in self.scratch.iter().zip(&mut self.cpus) {
            let busy = cpu.busy.advance(ticks.busy);
            let total = cpu.total.advance(ticks.total());
            let steal = cpu.steal.advance(ticks.steal);
            let (busy, total, steal) = (
                busy.map_err(|e| CollectError::Counter("cpu busy ticks", e))?,
                total.map_err(|e| CollectError::Counter("cpu total ticks", e))?,
                steal.map_err(|e| CollectError::Counter("cpu steal ticks", e))?,
            );
            // No ticks between reads carries no information; it is not 0 % busy.
            if let (Some(busy), Some(total), Some(steal)) = (busy, total, steal)
                && total > 0
            {
                cpu.utilization.push(100.0 * busy as f64 / total as f64)?;
                cpu.steal_percent
                    .push(100.0 * steal as f64 / total as f64)?;
            }
        }
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        for cpu in &mut self.cpus {
            cpu.utilization.close(ctx, out);
            cpu.steal_percent.close(ctx, out);
        }
    }
}
