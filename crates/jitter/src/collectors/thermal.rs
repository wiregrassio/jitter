//! `thermal`: temperature per thermal zone, selected by the zone's `type`.
//!
//! Zones are numbered in probe order, which can change between boots and kernels, so config
//! names a zone by its `type` and startup resolves it to exactly one `thermal_zoneN`. Many cloud
//! VMs expose no thermal zones at all; there, this collector stays disabled.

use super::{CloseContext, CollectError, Collector, Dist, capacity, period};
use crate::source::{ReadError, Source};
use jitter_core::line::Record;
use jitter_core::parse::scalar;
use std::path::Path;
use std::time::{Duration, Instant};

struct Zone {
    source: Source,
    celsius: Dist,
}

/// The thermal collector.
pub struct ThermalCollector {
    period: Duration,
    zones: Vec<Zone>,
}

impl ThermalCollector {
    /// Resolve each configured zone type to exactly one `<sys>/class/thermal/thermal_zoneN`.
    pub fn open(
        sys: &Path,
        types: &[String],
        hz: u32,
        window_seconds: u32,
    ) -> Result<ThermalCollector, CollectError> {
        let root = sys.join("class/thermal");
        let entries = std::fs::read_dir(&root).map_err(|error| {
            CollectError::Read(ReadError {
                path: root.clone(),
                error,
            })
        })?;
        let mut found: Vec<(String, std::path::PathBuf)> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                CollectError::Read(ReadError {
                    path: root.clone(),
                    error,
                })
            })?;
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with("thermal_zone")
            {
                continue;
            }
            let mut type_src = Source::open(entry.path().join("type"))?;
            found.push((type_src.read()?.trim().to_string(), entry.path()));
        }
        let cap = capacity(hz, window_seconds);
        let mut zones = Vec::with_capacity(types.len());
        for wanted in types {
            let matches: Vec<_> = found.iter().filter(|(t, _)| t == wanted).collect();
            let dir = match matches.as_slice() {
                [(_, dir)] => dir,
                [] => {
                    let present: Vec<&str> = found.iter().map(|(t, _)| t.as_str()).collect();
                    return Err(CollectError::Missing(format!(
                        "thermal zone type `{wanted}` (present: {present:?})"
                    )));
                }
                _ => {
                    return Err(CollectError::Missing(format!(
                        "a unique thermal zone of type `{wanted}`: {} zones share it",
                        matches.len()
                    )));
                }
            };
            let mut source = Source::open(dir.join("temp"))?;
            scalar::parse_millicelsius("thermal temp", source.read()?)?;
            zones.push(Zone {
                source,
                celsius: Dist::new(
                    "system.thermal.temperature_celsius",
                    vec![("zone", wanted.clone())],
                    cap,
                ),
            });
        }
        Ok(ThermalCollector {
            period: period(hz),
            zones,
        })
    }
}

impl Collector for ThermalCollector {
    fn name(&self) -> &'static str {
        "thermal"
    }

    fn period(&self) -> Duration {
        self.period
    }

    fn sample(&mut self, _now: Instant) -> Result<(), CollectError> {
        for z in &mut self.zones {
            let c = scalar::parse_millicelsius("thermal temp", z.source.read()?)?;
            z.celsius.push(c)?;
        }
        Ok(())
    }

    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>) {
        for z in &mut self.zones {
            z.celsius.close(ctx, out);
        }
    }
}
