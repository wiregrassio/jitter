//! `disk`: per-device I/O rates and busy time from `/proc/diskstats`, and filesystem capacity
//! per mount from `statvfs`.

use super::{
    CloseContext, CollectError, Collector, Dist, Point, capacity, elapsed_seconds, int, period,
};
use crate::source::Source;
use crate::sys;
use jitter_core::counter::Last;
use jitter_core::line::Record;
use jitter_core::parse::diskstats::{self, Disk};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct PerDevice {
    name: String,
    last: [Last; 5],
    read_bytes: Dist,
    write_bytes: Dist,
    reads: Dist,
    writes: Dist,
    busy: Dist,
    in_flight: Dist,
}

struct PerMount {
    path: PathBuf,
    total: Point,
    available: Point,
}

/// The disk collector.
pub struct DiskCollector {
    source: Source,
    period: Duration,
    devices: Vec<PerDevice>,
    mounts: Vec<PerMount>,
    last_at: Option<Instant>,
    scratch: Vec<Disk>,
}

impl DiskCollector {
    /// Open `<proc>/diskstats`, check every configured device exists and every mount answers
    /// `statvfs`.
    pub fn open(
        proc: &Path,
        names: &[String],
        mount_paths: &[PathBuf],
        hz: u32,
        window_seconds: u32,
    ) -> Result<DiskCollector, CollectError> {
        let mut source = Source::open(proc.join("diskstats"))?;
        let mut scratch = Vec::new();
        diskstats::parse(source.read()?, &mut scratch)?;
        let cap = capacity(hz, window_seconds);
        let mut devices = Vec::with_capacity(names.len());
        for name in names {
            if !scratch.iter().any(|d| &d.name == name) {
                return Err(CollectError::Missing(format!(
                    "block device `{name}` in diskstats"
                )));
            }
            let tags = || vec![("device", name.clone())];
            devices.push(PerDevice {
                name: name.clone(),
                last: [Last::default(); 5],
                read_bytes: Dist::new("system.disk.read_bytes_per_second", tags(), cap),
                write_bytes: Dist::new("system.disk.write_bytes_per_second", tags(), cap),
                reads: Dist::new("system.disk.reads_per_second", tags(), cap),
                writes: Dist::new("system.disk.writes_per_second", tags(), cap),
                busy: Dist::new("system.disk.busy_percent", tags(), cap),
                in_flight: Dist::new("system.disk.in_flight", tags(), cap),
            });
        }
        let mut mounts = Vec::with_capacity(mount_paths.len());
        for path in mount_paths {
            sys::statvfs(path)
                .map_err(|e| CollectError::Missing(format!("mount `{}` ({e})", path.display())))?;
            let tags = || vec![("mount", path.display().to_string())];
            mounts.push(PerMount {
                path: path.clone(),
                total: Point::new("system.filesystem.total_bytes", tags()),
                available: Point::new("system.filesystem.available_bytes", tags()),
            });
        }
        Ok(DiskCollector {
            source,
            period: period(hz),
            devices,
            mounts,
            last_at: None,
            scratch,
        })
    }
}

impl Collector for DiskCollector {
    fn name(&self) -> &'static str {
        "disk"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, now: Instant) -> Result<(), CollectError> {
        if !self.devices.is_empty() {
            diskstats::parse(self.source.read()?, &mut self.scratch)?;
        }
        let secs = self.last_at.map(|t| elapsed_seconds(t, now));
        self.last_at = Some(now);
        for dev in &mut self.devices {
            let Some(d) = self.scratch.iter().find(|d| d.name == dev.name) else {
                return Err(CollectError::Missing(format!(
                    "block device `{}`, present at startup",
                    dev.name
                )));
            };
            let current = [d.read_bytes, d.written_bytes, d.reads, d.writes, d.busy_ms];
            let mut deltas = [None; 5];
            for (i, (last, value)) in dev.last.iter_mut().zip(current).enumerate() {
                deltas[i] = last
                    .advance(value)
                    .map_err(|e| CollectError::Counter("diskstats", e))?;
            }
            dev.in_flight.push(d.in_flight as f64)?;
            if let ([Some(rb), Some(wb), Some(r), Some(w), Some(busy)], Some(secs)) = (deltas, secs)
                && secs > 0.0
            {
                dev.read_bytes.push(rb as f64 / secs)?;
                dev.write_bytes.push(wb as f64 / secs)?;
                dev.reads.push(r as f64 / secs)?;
                dev.writes.push(w as f64 / secs)?;
                // busy_ms over elapsed ms; clamp nothing, so an accounting overshoot is visible.
                dev.busy.push(100.0 * busy as f64 / (secs * 1000.0))?;
            }
        }
        for m in &mut self.mounts {
            let cap = sys::statvfs(&m.path).map_err(|e| CollectError::Sys("statvfs", e))?;
            m.total.set(int(cap.total_bytes));
            m.available.set(int(cap.available_bytes));
        }
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        for d in &mut self.devices {
            d.read_bytes.close(ctx, out);
            d.write_bytes.close(ctx, out);
            d.reads.close(ctx, out);
            d.writes.close(ctx, out);
            d.busy.close(ctx, out);
            d.in_flight.close(ctx, out);
        }
        for m in &mut self.mounts {
            m.total.close(ctx, out);
            m.available.close(ctx, out);
        }
    }
}
