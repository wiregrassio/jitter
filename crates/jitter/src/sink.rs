//! The one sink. The driver hands it one serialized window at a time through a bounded queue;
//! a full queue drops the window and counts it, so a slow or absent store can never block
//! sampling or grow memory.
//!
//! The sink thread owns the store connection. A window is counted written only after the store
//! answers 2xx; a failed attempt is retried up to the configured limit, then the window is
//! dropped and counted. Nothing is spooled to disk: telemetry is the most droppable payload,
//! and a spool trades a bounded gap for unbounded local state.

use crate::config::{Influxdb, Sink as SinkConfig, SinkKind};
use crate::http::Endpoint;
use crate::log::{self, Level};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

/// One window, serialized.
pub struct Batch {
    /// Line protocol, newline-terminated lines.
    pub lines: String,
}

/// Counters the self report publishes. Cumulative since start.
#[derive(Debug, Default)]
pub struct SinkStats {
    /// Windows dropped because the queue was full or every attempt failed.
    pub dropped: AtomicU64,
    /// Attempts that failed.
    pub failed: AtomicU64,
    /// Windows confirmed by the store, or written to stdout.
    pub written: AtomicU64,
}

/// The driver's end.
pub struct SinkHandle {
    queue: Option<SyncSender<Batch>>,
    thread: Option<JoinHandle<()>>,
    stats: Arc<SinkStats>,
    was_dropping: bool,
}

/// A sink that cannot start.
#[derive(Debug)]
pub struct SinkError(pub String);

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sink: {}", self.0)
    }
}

enum Target {
    Influx {
        endpoint: Endpoint,
        path: String,
        auth: String,
        timeout: Duration,
        attempts: u32,
        retry_delay: Duration,
    },
    Stdout,
}

impl SinkHandle {
    /// Start the sink thread. The InfluxDB token is read from its environment variable here, so
    /// a missing token fails at startup.
    pub fn start(config: &SinkConfig) -> Result<SinkHandle, SinkError> {
        let target = match (config.kind, &config.influxdb) {
            (SinkKind::Stdout, _) => Target::Stdout,
            (SinkKind::Influxdb, Some(influx)) => influx_target(influx)?,
            (SinkKind::Influxdb, None) => {
                return Err(SinkError("influxdb sink without settings".into()));
            }
        };
        let (queue, rx) = sync_channel::<Batch>(config.queue_windows as usize);
        let stats = Arc::new(SinkStats::default());
        let thread_stats = Arc::clone(&stats);
        let thread = std::thread::Builder::new()
            .name("jitter-sink".into())
            .spawn(move || run(target, rx, &thread_stats))
            .map_err(|e| SinkError(format!("cannot start thread: {e}")))?;
        Ok(SinkHandle {
            queue: Some(queue),
            thread: Some(thread),
            stats,
            was_dropping: false,
        })
    }

    /// The shared counters.
    pub fn stats(&self) -> Arc<SinkStats> {
        Arc::clone(&self.stats)
    }

    /// Offer a window without blocking. A full queue drops it and counts the drop; the first
    /// drop after a success is logged, so an outage is one line, not one per second.
    pub fn offer(&mut self, batch: Batch) {
        let Some(queue) = &self.queue else { return };
        match queue.try_send(batch) {
            Ok(()) => self.was_dropping = false,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                if !self.was_dropping {
                    log::event(Level::Warn, "window_dropped", &[("reason", &"queue full")]);
                    self.was_dropping = true;
                }
            }
        }
    }

    /// Close the queue and wait for the thread to finish what is queued. Bounded by the queue
    /// length times the per-window retry budget.
    pub fn finish(mut self) {
        drop(self.queue.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn influx_target(influx: &Influxdb) -> Result<Target, SinkError> {
    let token = std::env::var(&influx.token_env).map_err(|_| {
        SinkError(format!(
            "environment variable {} is not set",
            influx.token_env
        ))
    })?;
    if token.is_empty() || token.starts_with(crate::config::SENTINEL_PREFIX) {
        return Err(SinkError(format!(
            "{} holds no real token",
            influx.token_env
        )));
    }
    let endpoint = Endpoint::parse_url(&influx.url).map_err(SinkError)?;
    Ok(Target::Influx {
        endpoint,
        path: format!(
            "/api/v2/write?org={}&bucket={}&precision=ns",
            influx.org, influx.bucket
        ),
        auth: format!("Token {token}"),
        timeout: Duration::from_millis(influx.timeout_ms),
        attempts: influx.max_attempts,
        retry_delay: Duration::from_millis(influx.retry_delay_ms),
    })
}

fn run(target: Target, rx: Receiver<Batch>, stats: &SinkStats) {
    let stdout = std::io::stdout();
    while let Ok(batch) = rx.recv() {
        match &target {
            Target::Stdout => {
                let mut out = stdout.lock();
                if let Err(e) = out
                    .write_all(batch.lines.as_bytes())
                    .and_then(|_| out.flush())
                {
                    // The reader of stdout is gone. Nothing downstream will see another record,
                    // so running on would be the "up and writing nothing" failure.
                    log::event(Level::Error, "stdout_closed", &[("error", &e)]);
                    std::process::exit(crate::EXIT_SINK);
                }
                stats.written.fetch_add(1, Ordering::Relaxed);
            }
            Target::Influx {
                endpoint,
                path,
                auth,
                timeout,
                attempts,
                retry_delay,
            } => {
                let headers = [
                    ("Authorization", auth.as_str()),
                    ("Content-Type", "text/plain; charset=utf-8"),
                ];
                let mut landed = false;
                for attempt in 1..=*attempts {
                    match endpoint.post(path, &headers, batch.lines.as_bytes(), *timeout) {
                        Ok(()) => {
                            landed = true;
                            break;
                        }
                        Err(e) => {
                            stats.failed.fetch_add(1, Ordering::Relaxed);
                            log::event(
                                Level::Warn,
                                "write_failed",
                                &[("attempt", &attempt), ("error", &e)],
                            );
                            if attempt < *attempts {
                                std::thread::sleep(*retry_delay);
                            }
                        }
                    }
                }
                if landed {
                    stats.written.fetch_add(1, Ordering::Relaxed);
                } else {
                    stats.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

/// Validate sink settings that depend on the environment, without starting anything.
pub fn check(config: &SinkConfig) -> Result<(), SinkError> {
    match (config.kind, &config.influxdb) {
        (SinkKind::Stdout, _) => Ok(()),
        (SinkKind::Influxdb, Some(influx)) => influx_target(influx).map(|_| ()),
        (SinkKind::Influxdb, None) => Err(SinkError("influxdb sink without settings".into())),
    }
}
