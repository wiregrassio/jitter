//! The closed collector set and the two stream shapes every collector is built from.
//!
//! A collector does two things: `sample` reads its source and pushes values into its streams,
//! and `close` reduces each stream to a record at the end of a window. It knows nothing about
//! timing or transport; the driver owns both.

use crate::config::Config;
use crate::source::ReadError;
use jitter_core::counter::Backwards;
use jitter_core::line::{FieldValue, Payload, Record};
use jitter_core::parse::ParseError;
use jitter_core::stats::{Window, WindowError};
use std::time::{Duration, Instant};

pub mod cpu;
pub mod disk;
pub mod memory;
pub mod network;
pub mod pressure;
pub mod sched;
pub mod thermal;
#[cfg(target_os = "linux")]
pub mod timesync;

/// Every way a collector can fail. All of them are fatal: the targets are kernels with stable
/// interfaces, so a failure at runtime is not transient, and a partly running collector set
/// looks like "nothing changed" to anyone reading the store.
#[derive(Debug)]
pub enum CollectError {
    /// A source file could not be opened or read.
    Read(ReadError),
    /// A source file had the wrong shape.
    Parse(ParseError),
    /// A counter went backwards.
    Counter(&'static str, Backwards),
    /// A stream received more samples than its window holds, or a non-finite value.
    Window(&'static str, WindowError),
    /// Something configured is not present, or something present at startup has gone.
    Missing(String),
    /// A system call failed.
    Sys(&'static str, std::io::Error),
}

impl std::fmt::Display for CollectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectError::Read(e) => write!(f, "read {e}"),
            CollectError::Parse(e) => write!(f, "parse {e}"),
            CollectError::Counter(what, e) => write!(f, "{what}: {e}"),
            CollectError::Window(what, e) => write!(f, "{what}: {e}"),
            CollectError::Missing(what) => write!(f, "missing {what}"),
            CollectError::Sys(call, e) => write!(f, "{call}: {e}"),
        }
    }
}

impl From<ReadError> for CollectError {
    fn from(e: ReadError) -> Self {
        CollectError::Read(e)
    }
}

impl From<ParseError> for CollectError {
    fn from(e: ParseError) -> Self {
        CollectError::Parse(e)
    }
}

/// The interface the driver runs.
pub trait Collector {
    /// Collector name, as in the config table.
    fn name(&self) -> &'static str;
    /// Time between samples.
    fn period(&self) -> Duration;
    /// Read the source once. `now` is taken just before the read and is the time the sample
    /// describes.
    fn sample(&mut self, now: Instant) -> Result<(), CollectError>;
    /// Reduce every stream into `out` and start a new window.
    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>);
}

/// What every record of a window shares.
#[derive(Debug, Clone)]
pub struct CloseContext {
    /// The `service` tag, from the package name.
    pub service: String,
    /// The `host` tag, from config.
    pub host: String,
    /// Window end, nanoseconds since the Unix epoch.
    pub timestamp_ns: i64,
}

impl CloseContext {
    fn tags(&self, own: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
        let mut tags = Vec::with_capacity(2 + own.len());
        tags.push(("service", self.service.clone()));
        tags.push(("host", self.host.clone()));
        tags.extend(own.iter().cloned());
        tags
    }
}

/// A stream reduced to a distribution each window.
#[derive(Debug)]
pub struct Dist {
    measurement: &'static str,
    tags: Vec<(&'static str, String)>,
    window: Window,
}

impl Dist {
    /// A stream holding at most `capacity` samples per window.
    pub fn new(
        measurement: &'static str,
        tags: Vec<(&'static str, String)>,
        capacity: usize,
    ) -> Dist {
        Dist {
            measurement,
            tags,
            window: Window::with_capacity(capacity),
        }
    }

    /// Add a sample.
    pub fn push(&mut self, value: f64) -> Result<(), CollectError> {
        self.window
            .push(value)
            .map_err(|e| CollectError::Window(self.measurement, e))
    }

    /// Emit the window's record, if it had samples.
    pub fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        if let Some(summary) = self.window.close() {
            out.push(Record {
                measurement: self.measurement,
                tags: ctx.tags(&self.tags),
                payload: Payload::Distribution(summary),
                timestamp_ns: ctx.timestamp_ns,
            });
        }
    }
}

/// A stream that reports its latest value each window: counters, capacities, states.
#[derive(Debug)]
pub struct Point {
    measurement: &'static str,
    tags: Vec<(&'static str, String)>,
    latest: Option<FieldValue>,
}

impl Point {
    /// A new point stream.
    pub fn new(measurement: &'static str, tags: Vec<(&'static str, String)>) -> Point {
        Point {
            measurement,
            tags,
            latest: None,
        }
    }

    /// Replace the latest value.
    pub fn set(&mut self, value: FieldValue) {
        self.latest = Some(value);
    }

    /// Emit the latest value, if one was set this window.
    pub fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        if let Some(value) = self.latest.take() {
            out.push(Record {
                measurement: self.measurement,
                tags: ctx.tags(&self.tags),
                payload: Payload::Point(value),
                timestamp_ns: ctx.timestamp_ns,
            });
        }
    }
}

/// A counter value as an integer field, saturating at `i64::MAX` (2^63 bytes is 8 EiB).
pub fn int(v: u64) -> FieldValue {
    FieldValue::Int(i64::try_from(v).unwrap_or(i64::MAX))
}

/// Samples one stream holds per window at `hz`.
pub fn capacity(hz: u32, window_seconds: u32) -> usize {
    (hz as usize) * (window_seconds as usize)
}

/// Period of a rate in Hz. Validated rates divide one second exactly.
pub fn period(hz: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(hz.max(1)))
}

/// Elapsed seconds between two sample instants, for rates.
pub fn elapsed_seconds(previous: Instant, now: Instant) -> f64 {
    now.saturating_duration_since(previous).as_secs_f64()
}

/// Build every enabled collector. Each one opens its sources here, so anything missing fails
/// before the first sample.
pub fn build(config: &Config) -> Result<Vec<Box<dyn Collector>>, CollectError> {
    let c = &config.collectors;
    let w = config.window.seconds;
    let proc = &config.paths.proc;
    let mut out: Vec<Box<dyn Collector>> = Vec::new();
    if c.cpu.enabled {
        out.push(Box::new(cpu::Cpu::open(proc, c.cpu.sample_hz, w)?));
    }
    if c.sched.enabled {
        out.push(Box::new(sched::Sched::open(proc, c.sched.sample_hz, w)?));
    }
    if c.memory.enabled {
        out.push(Box::new(memory::Memory::open(proc, c.memory.sample_hz, w)?));
    }
    if c.pressure.enabled {
        out.push(Box::new(pressure::PressureCollector::open(
            proc,
            c.pressure.sample_hz,
            w,
        )?));
    }
    if c.network.enabled {
        out.push(Box::new(network::Network::open(
            proc,
            &c.network.interfaces,
            c.network.sample_hz,
            w,
        )?));
    }
    if c.disk.enabled {
        out.push(Box::new(disk::DiskCollector::open(
            proc,
            &c.disk.devices,
            &c.disk.mounts,
            c.disk.sample_hz,
            w,
        )?));
    }
    if c.thermal.enabled {
        out.push(Box::new(thermal::ThermalCollector::open(
            &config.paths.sys,
            &c.thermal.zones,
            c.thermal.sample_hz,
            w,
        )?));
    }
    if c.timesync.enabled {
        #[cfg(target_os = "linux")]
        out.push(Box::new(timesync::Timesync::open(c.timesync.sample_hz, w)?));
        #[cfg(not(target_os = "linux"))]
        return Err(CollectError::Missing("adjtimex (Linux only)".into()));
    }
    Ok(out)
}
