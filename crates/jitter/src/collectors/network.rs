//! `network`: per-interface throughput, errors, and drops.
//!
//! Reads `<proc>/1/net/dev`, PID 1's view, not `<proc>/net/dev`, which is the reader's own
//! network namespace. On a host the two are the same; in a container with the host's procfs
//! mounted, PID 1's view is the host's interfaces without host networking.

use super::{
    CloseContext, CollectError, Collector, Dist, Point, capacity, elapsed_seconds, int, period,
};
use crate::source::Source;
use jitter_core::counter::Last;
use jitter_core::line::Record;
use jitter_core::parse::net_dev::{self, Interface};
use std::path::Path;
use std::time::{Duration, Instant};

struct PerInterface {
    name: String,
    rx: Last,
    tx: Last,
    rx_rate: Dist,
    tx_rate: Dist,
    rx_errors: Point,
    tx_errors: Point,
    rx_dropped: Point,
    tx_dropped: Point,
}

/// The network collector.
pub struct Network {
    source: Source,
    period: Duration,
    interfaces: Vec<PerInterface>,
    last_at: Option<Instant>,
    scratch: Vec<Interface>,
}

impl Network {
    /// Open `<proc>/1/net/dev` and check every configured interface exists.
    pub fn open(
        proc: &Path,
        names: &[String],
        hz: u32,
        window_seconds: u32,
    ) -> Result<Network, CollectError> {
        let mut source = Source::open(proc.join("1/net/dev"))?;
        let mut scratch = Vec::new();
        net_dev::parse(source.read()?, &mut scratch)?;
        let cap = capacity(hz, window_seconds);
        let mut interfaces = Vec::with_capacity(names.len());
        for name in names {
            if !scratch.iter().any(|i| &i.name == name) {
                let present: Vec<&str> = scratch.iter().map(|i| i.name.as_str()).collect();
                return Err(CollectError::Missing(format!(
                    "network interface `{name}` (present: {present:?})"
                )));
            }
            let tags = || vec![("interface", name.clone())];
            interfaces.push(PerInterface {
                name: name.clone(),
                rx: Last::default(),
                tx: Last::default(),
                rx_rate: Dist::new("system.network.rx_bytes_per_second", tags(), cap),
                tx_rate: Dist::new("system.network.tx_bytes_per_second", tags(), cap),
                rx_errors: Point::new("system.network.rx_errors_total", tags()),
                tx_errors: Point::new("system.network.tx_errors_total", tags()),
                rx_dropped: Point::new("system.network.rx_dropped_total", tags()),
                tx_dropped: Point::new("system.network.tx_dropped_total", tags()),
            });
        }
        Ok(Network {
            source,
            period: period(hz),
            interfaces,
            last_at: None,
            scratch,
        })
    }
}

impl Collector for Network {
    fn name(&self) -> &'static str {
        "network"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, now: Instant) -> Result<(), CollectError> {
        net_dev::parse(self.source.read()?, &mut self.scratch)?;
        let secs = self.last_at.map(|t| elapsed_seconds(t, now));
        self.last_at = Some(now);
        for iface in &mut self.interfaces {
            let Some(c) = self.scratch.iter().find(|i| i.name == iface.name) else {
                return Err(CollectError::Missing(format!(
                    "network interface `{}`, present at startup",
                    iface.name
                )));
            };
            let rx = iface
                .rx
                .advance(c.rx_bytes)
                .map_err(|e| CollectError::Counter("rx_bytes", e))?;
            let tx = iface
                .tx
                .advance(c.tx_bytes)
                .map_err(|e| CollectError::Counter("tx_bytes", e))?;
            if let (Some(rx), Some(tx), Some(secs)) = (rx, tx, secs)
                && secs > 0.0
            {
                iface.rx_rate.push(rx as f64 / secs)?;
                iface.tx_rate.push(tx as f64 / secs)?;
            }
            iface.rx_errors.set(int(c.rx_errors));
            iface.tx_errors.set(int(c.tx_errors));
            iface.rx_dropped.set(int(c.rx_dropped));
            iface.tx_dropped.set(int(c.tx_dropped));
        }
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        for i in &mut self.interfaces {
            i.rx_rate.close(ctx, out);
            i.tx_rate.close(ctx, out);
            i.rx_errors.close(ctx, out);
            i.tx_errors.close(ctx, out);
            i.rx_dropped.close(ctx, out);
            i.tx_dropped.close(ctx, out);
        }
    }
}
