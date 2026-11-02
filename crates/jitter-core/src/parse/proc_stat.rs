//! `/proc/stat` CPU lines: time per state, in USER_HZ ticks.
//!
//! Tick units are the reason Jitter caps the CPU collector's sample rate: with USER_HZ = 100 a
//! 10 ms sample sees zero or one tick per CPU, so its utilization can only read 0 % or 100 %.

use super::{ParseError, numbers};

const SOURCE: &str = "/proc/stat";

/// Cumulative ticks for one CPU, or for all CPUs when `cpu` is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTicks {
    /// CPU index from `cpuN`; `None` for the aggregate `cpu` line.
    pub cpu: Option<u32>,
    /// user + nice + system + irq + softirq. Guest time is already inside user and nice.
    pub busy: u64,
    /// idle + iowait.
    pub idle: u64,
    /// Time a hypervisor ran something else while this vCPU wanted to run.
    pub steal: u64,
}

impl CpuTicks {
    /// Every tick this CPU accounted.
    pub fn total(&self) -> u64 {
        self.busy + self.idle + self.steal
    }
}

/// Parse every `cpu` line of `/proc/stat` into `out`, aggregate first, then per CPU in file order.
pub fn parse(text: &str, out: &mut Vec<CpuTicks>) -> Result<(), ParseError> {
    out.clear();
    let mut fields = Vec::with_capacity(10);
    for (i, line) in text.lines().enumerate() {
        let mut parts = line.split_ascii_whitespace();
        let Some(label) = parts.next() else { continue };
        let Some(suffix) = label.strip_prefix("cpu") else {
            continue;
        };
        let cpu = if suffix.is_empty() {
            None
        } else {
            Some(suffix.parse::<u32>().map_err(|_| {
                ParseError::new(SOURCE, i + 1, format!("`{label}` is not a cpu label"))
            })?)
        };
        numbers(SOURCE, i + 1, parts, &mut fields)?;
        // user nice system idle iowait irq softirq steal [guest guest_nice]
        if fields.len() < 8 {
            return Err(ParseError::new(
                SOURCE,
                i + 1,
                format!("{} fields, need at least 8", fields.len()),
            ));
        }
        out.push(CpuTicks {
            cpu,
            busy: fields[0] + fields[1] + fields[2] + fields[5] + fields[6],
            idle: fields[3] + fields[4],
            steal: fields[7],
        });
    }
    match out.first() {
        Some(first) if first.cpu.is_none() => Ok(()),
        _ => Err(ParseError::new(SOURCE, 0, "no aggregate `cpu` line first")),
    }
}
