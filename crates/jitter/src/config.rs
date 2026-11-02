//! Configuration: every key required, every value validated before anything starts.
//!
//! There are no defaults. A missing key, an unknown key, or an out-of-range value is a named
//! startup failure, because a plausible default runs on a wrong value while looking healthy.

use serde::Deserialize;
use std::path::PathBuf;

/// Upper bound on any collector's sample rate. Above this the loop's own wake-ups dominate the
/// footprint it exists to keep small.
pub const MAX_SAMPLE_HZ: u32 = 1000;
/// Upper bound on the window length.
pub const MAX_WINDOW_SECONDS: u32 = 60;
/// The fewest scheduler ticks one CPU sample may span. At USER_HZ = 100 and 10 ticks per sample,
/// one tick moves utilization by 10 percentage points; any shorter and the distribution is mostly
/// quantization.
pub const MIN_CPU_TICKS_PER_SAMPLE: u32 = 10;
/// Values starting with this are placeholders from the example config and are refused.
pub const SENTINEL_PREFIX: &str = "SENTINEL";

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Who is reporting.
    pub identity: Identity,
    /// Where the kernel's interfaces are mounted.
    pub paths: Paths,
    /// Window length.
    pub window: WindowConfig,
    /// Sampling loop settings.
    pub driver: Driver,
    /// The closed collector set.
    pub collectors: Collectors,
    /// Where records go.
    pub sink: Sink,
}

/// Identity tags on every record.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// The `host` tag. Required; never inferred from the hostname, which differs inside a
    /// container.
    pub host: String,
}

/// Mount points of procfs and sysfs. In a container these are the host's, bind-mounted.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// procfs root, normally `/proc`.
    pub proc: PathBuf,
    /// sysfs root, normally `/sys`.
    pub sys: PathBuf,
}

/// Window settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowConfig {
    /// Window length in whole seconds. Windows start on wall-clock second boundaries.
    pub seconds: u32,
}

/// Sampling loop settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    /// The loop thread's timer slack in nanoseconds (`PR_SET_TIMERSLACK`). The kernel default of
    /// 50 000 lets every wake-up land up to 50 us late by design.
    pub timer_slack_ns: u64,
}

/// One collector's common keys.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    /// Run this collector.
    pub enabled: bool,
    /// Samples per second.
    pub sample_hz: u32,
}

/// Network collector keys.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// Run this collector.
    pub enabled: bool,
    /// Samples per second.
    pub sample_hz: u32,
    /// Interface names, each required to exist at startup.
    pub interfaces: Vec<String>,
}

/// Disk collector keys.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disk {
    /// Run this collector.
    pub enabled: bool,
    /// Samples per second.
    pub sample_hz: u32,
    /// Block device names from `/proc/diskstats`, each required to exist at startup.
    pub devices: Vec<String>,
    /// Mount paths whose capacity is reported.
    pub mounts: Vec<PathBuf>,
}

/// Thermal collector keys.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thermal {
    /// Run this collector.
    pub enabled: bool,
    /// Samples per second.
    pub sample_hz: u32,
    /// Thermal zone `type` names, each required to match exactly one zone.
    pub zones: Vec<String>,
}

/// The self collector has no rate: it reports once per window.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfReport {
    /// Report Jitter's own footprint and timing.
    pub enabled: bool,
}

/// The closed collector set. Adding a member is a design decision, not a config change.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collectors {
    /// `/proc/stat`.
    pub cpu: Rate,
    /// `/proc/schedstat`.
    pub sched: Rate,
    /// `/proc/meminfo`, `/proc/vmstat`.
    pub memory: Rate,
    /// `/proc/pressure`.
    pub pressure: Rate,
    /// `/proc/1/net/dev`.
    pub network: Network,
    /// `/proc/diskstats`, `statvfs`.
    pub disk: Disk,
    /// `/sys/class/thermal`.
    pub thermal: Thermal,
    /// `adjtimex`.
    pub timesync: Rate,
    /// Jitter itself.
    #[serde(rename = "self")]
    pub self_report: SelfReport,
}

/// Sink selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinkKind {
    /// InfluxDB 2.x HTTP write API.
    Influxdb,
    /// Line protocol on stdout, one window per flush.
    Stdout,
}

/// Sink settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sink {
    /// Which sink.
    pub kind: SinkKind,
    /// Windows that may wait for the sink before new ones are dropped.
    pub queue_windows: u32,
    /// Required when `kind = "influxdb"`, refused otherwise.
    pub influxdb: Option<Influxdb>,
}

/// InfluxDB 2.x settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Influxdb {
    /// `http://host:port`. Plain HTTP only: the store is local.
    pub url: String,
    /// Organization name.
    pub org: String,
    /// Bucket name.
    pub bucket: String,
    /// Environment variable holding the API token. The token never appears in the file.
    pub token_env: String,
    /// Connect, write, and read timeout per attempt.
    pub timeout_ms: u64,
    /// Attempts per window before it is dropped and counted.
    pub max_attempts: u32,
    /// Pause between attempts.
    pub retry_delay_ms: u64,
}

/// A configuration that cannot run.
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read.
    Read(std::io::Error),
    /// The file is not valid TOML for this schema: missing, unknown, or mistyped keys.
    Schema(String),
    /// A value is out of range or contradicts another.
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(e) => write!(f, "config: cannot read: {e}"),
            ConfigError::Schema(e) => write!(f, "config: {e}"),
            ConfigError::Invalid(e) => write!(f, "config: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Kernel facts validation depends on, passed in so validation stays testable off-target.
#[derive(Debug, Clone, Copy)]
pub struct Kernel {
    /// `sysconf(_SC_CLK_TCK)`, the unit of `/proc/stat`.
    pub user_hz: u32,
}

impl Config {
    /// Read, parse, and validate.
    pub fn load(path: &std::path::Path, kernel: Kernel) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(ConfigError::Read)?;
        Config::parse(&text, kernel)
    }

    /// Parse and validate text.
    pub fn parse(text: &str, kernel: Kernel) -> Result<Config, ConfigError> {
        let config: Config =
            toml::from_str(text).map_err(|e| ConfigError::Schema(e.to_string()))?;
        config.validate(kernel)?;
        Ok(config)
    }

    fn validate(&self, kernel: Kernel) -> Result<(), ConfigError> {
        let bad = |msg: String| Err(ConfigError::Invalid(msg));

        check_tag_value("identity.host", &self.identity.host)?;
        if !(1..=MAX_WINDOW_SECONDS).contains(&self.window.seconds) {
            return bad(format!(
                "window.seconds = {} is outside 1..={MAX_WINDOW_SECONDS}",
                self.window.seconds
            ));
        }

        let c = &self.collectors;
        let rated = [
            ("cpu", c.cpu.enabled, c.cpu.sample_hz),
            ("sched", c.sched.enabled, c.sched.sample_hz),
            ("memory", c.memory.enabled, c.memory.sample_hz),
            ("pressure", c.pressure.enabled, c.pressure.sample_hz),
            ("network", c.network.enabled, c.network.sample_hz),
            ("disk", c.disk.enabled, c.disk.sample_hz),
            ("thermal", c.thermal.enabled, c.thermal.sample_hz),
            ("timesync", c.timesync.enabled, c.timesync.sample_hz),
        ];
        if !rated.iter().any(|(_, on, _)| *on) {
            return bad("every collector is disabled".into());
        }
        for (name, _, hz) in rated {
            if !(1..=MAX_SAMPLE_HZ).contains(&hz) {
                return bad(format!(
                    "collectors.{name}.sample_hz = {hz} is outside 1..={MAX_SAMPLE_HZ}"
                ));
            }
            // Deadlines are whole nanoseconds on a grid anchored at a second boundary. A rate
            // that does not divide a second would let the grid drift against the windows.
            if 1_000_000_000 % hz != 0 {
                return bad(format!(
                    "collectors.{name}.sample_hz = {hz} does not divide one second in whole nanoseconds"
                ));
            }
        }
        if c.cpu.enabled && c.cpu.sample_hz * MIN_CPU_TICKS_PER_SAMPLE > kernel.user_hz {
            return bad(format!(
                "collectors.cpu.sample_hz = {} is too fast for /proc/stat: at USER_HZ = {} a sample \
                 spans {} ticks and needs at least {MIN_CPU_TICKS_PER_SAMPLE}, so the maximum is {} Hz. \
                 Use collectors.sched for finer timing.",
                c.cpu.sample_hz,
                kernel.user_hz,
                kernel.user_hz / c.cpu.sample_hz,
                kernel.user_hz / MIN_CPU_TICKS_PER_SAMPLE
            ));
        }
        if c.network.enabled && c.network.interfaces.is_empty() {
            return bad("collectors.network is enabled with no interfaces".into());
        }
        for name in &c.network.interfaces {
            check_tag_value("collectors.network.interfaces", name)?;
        }
        if c.disk.enabled && c.disk.devices.is_empty() && c.disk.mounts.is_empty() {
            return bad("collectors.disk is enabled with no devices and no mounts".into());
        }
        for name in &c.disk.devices {
            check_tag_value("collectors.disk.devices", name)?;
        }
        for mount in &c.disk.mounts {
            if !mount.is_absolute() {
                return bad(format!(
                    "collectors.disk.mounts: `{}` is not absolute",
                    mount.display()
                ));
            }
        }
        if c.thermal.enabled && c.thermal.zones.is_empty() {
            return bad("collectors.thermal is enabled with no zones".into());
        }
        for zone in &c.thermal.zones {
            check_tag_value("collectors.thermal.zones", zone)?;
        }

        if self.sink.queue_windows == 0 {
            return bad("sink.queue_windows must be at least 1".into());
        }
        match (self.sink.kind, &self.sink.influxdb) {
            (SinkKind::Influxdb, None) => {
                return bad("sink.kind = \"influxdb\" needs a [sink.influxdb] table".into());
            }
            (SinkKind::Stdout, Some(_)) => {
                return bad(
                    "sink.kind = \"stdout\" with a [sink.influxdb] table: remove one".into(),
                );
            }
            (SinkKind::Influxdb, Some(influx)) => influx.validate()?,
            (SinkKind::Stdout, None) => {}
        }
        Ok(())
    }

    /// Kernel facts for this host.
    pub fn kernel_facts() -> Kernel {
        Kernel {
            user_hz: crate::sys::user_hz(),
        }
    }
}

impl Influxdb {
    fn validate(&self) -> Result<(), ConfigError> {
        crate::http::Endpoint::parse_url(&self.url).map_err(ConfigError::Invalid)?;
        for (key, value) in [("org", &self.org), ("bucket", &self.bucket)] {
            let ok = !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
            if !ok || value.starts_with(SENTINEL_PREFIX) {
                return Err(ConfigError::Invalid(format!(
                    "sink.influxdb.{key} = `{value}` must be set, using letters, digits, `-`, `_`, `.`"
                )));
            }
        }
        if self.token_env.is_empty() {
            return Err(ConfigError::Invalid(
                "sink.influxdb.token_env is empty".into(),
            ));
        }
        if self.timeout_ms == 0 || self.max_attempts == 0 {
            return Err(ConfigError::Invalid(
                "sink.influxdb.timeout_ms and max_attempts must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

fn check_tag_value(key: &str, value: &str) -> Result<(), ConfigError> {
    if value.is_empty() || value.starts_with(SENTINEL_PREFIX) || value.contains(['\n', '\r']) {
        return Err(ConfigError::Invalid(format!(
            "{key} = `{value}` must be set to a real, single-line value"
        )));
    }
    Ok(())
}
