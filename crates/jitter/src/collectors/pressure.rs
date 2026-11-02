//! `pressure`: the share of wall time tasks stalled on CPU, memory, and I/O (PSI).
//!
//! Needs a kernel built with `CONFIG_PSI` and not booted with `psi=0`. Without it the
//! collector refuses to start; enabling it is a claim that the kernel has it.

use super::{CloseContext, CollectError, Collector, Dist, capacity, elapsed_seconds, period};
use crate::source::Source;
use jitter_core::counter::Last;
use jitter_core::line::Record;
use jitter_core::parse::pressure;
use std::path::Path;
use std::time::{Duration, Instant};

const RESOURCES: [&str; 3] = ["cpu", "memory", "io"];

struct Resource {
    source: Source,
    some: Last,
    full: Option<Last>,
    some_percent: Dist,
    full_percent: Option<Dist>,
}

/// The pressure collector.
pub struct PressureCollector {
    period: Duration,
    resources: Vec<Resource>,
    last_at: Option<Instant>,
}

impl PressureCollector {
    /// Open `<proc>/pressure/{cpu,memory,io}`. A `full` line present at startup must stay present.
    pub fn open(
        proc: &Path,
        hz: u32,
        window_seconds: u32,
    ) -> Result<PressureCollector, CollectError> {
        let cap = capacity(hz, window_seconds);
        let mut resources = Vec::with_capacity(RESOURCES.len());
        for name in RESOURCES {
            let path = proc.join("pressure").join(name);
            let mut source = Source::open(&path).map_err(|e| {
                CollectError::Missing(format!(
                    "{} ({}): the kernel has no PSI; build with CONFIG_PSI or disable collectors.pressure",
                    path.display(),
                    e.error
                ))
            })?;
            let first = pressure::parse(source.read()?)?;
            let tags =
                |kind: &str| vec![("resource", name.to_string()), ("kind", kind.to_string())];
            resources.push(Resource {
                source,
                some: Last::default(),
                full: first.full_us.map(|_| Last::default()),
                some_percent: Dist::new("system.pressure.stall_percent", tags("some"), cap),
                full_percent: first
                    .full_us
                    .map(|_| Dist::new("system.pressure.stall_percent", tags("full"), cap)),
            });
        }
        Ok(PressureCollector {
            period: period(hz),
            resources,
            last_at: None,
        })
    }
}

impl Collector for PressureCollector {
    fn name(&self) -> &'static str {
        "pressure"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, now: Instant) -> Result<(), CollectError> {
        let elapsed_us = self.last_at.map(|t| elapsed_seconds(t, now) * 1e6);
        self.last_at = Some(now);
        for r in &mut self.resources {
            let p = pressure::parse(r.source.read()?)?;
            let some = r
                .some
                .advance(p.some_us)
                .map_err(|e| CollectError::Counter("pressure some", e))?;
            if let (Some(d), Some(us)) = (some, elapsed_us)
                && us > 0.0
            {
                r.some_percent.push(100.0 * d as f64 / us)?;
            }
            if let (Some(last), Some(dist)) = (&mut r.full, &mut r.full_percent) {
                let total = p.full_us.ok_or_else(|| {
                    CollectError::Missing(format!(
                        "`full` line in {} that was present at startup",
                        r.source.path().display()
                    ))
                })?;
                let d = last
                    .advance(total)
                    .map_err(|e| CollectError::Counter("pressure full", e))?;
                if let (Some(d), Some(us)) = (d, elapsed_us)
                    && us > 0.0
                {
                    dist.push(100.0 * d as f64 / us)?;
                }
            }
        }
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        for r in &mut self.resources {
            r.some_percent.close(ctx, out);
            if let Some(full) = &mut r.full_percent {
                full.close(ctx, out);
            }
        }
    }
}
