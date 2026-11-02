//! `/proc/schedstat` per-CPU lines: scheduler run and wait time, in nanoseconds.
//!
//! Units are nanoseconds, but the counters advance only when a task leaves the CPU. A task that
//! runs without switching out is invisible here until it does, so nanosecond units are not
//! nanosecond resolution. Jitter reports these beside the tick-based `/proc/stat` figures rather
//! than in place of them.

use super::{ParseError, numbers};

const SOURCE: &str = "/proc/schedstat";

/// Versions whose `cpuN` line layout this parser has been checked against.
pub const KNOWN_VERSIONS: [u32; 3] = [15, 16, 17];

/// Cumulative scheduler time for one CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSched {
    /// CPU index from `cpuN`.
    pub cpu: u32,
    /// Nanoseconds tasks other than idle ran on this CPU (field 7).
    pub running_ns: u64,
    /// Nanoseconds runnable tasks waited in this CPU's run queue (field 8).
    pub waiting_ns: u64,
}

/// Parse every `cpuN` line into `out`. Fails on a missing or unknown `version` line: a layout
/// change must be a loud failure, not a silent read of the wrong column.
pub fn parse(text: &str, out: &mut Vec<CpuSched>) -> Result<(), ParseError> {
    out.clear();
    let mut lines = text.lines().enumerate();
    let version = match lines.next() {
        Some((_, line)) => line
            .strip_prefix("version ")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .ok_or_else(|| ParseError::new(SOURCE, 1, "first line is not `version N`"))?,
        None => return Err(ParseError::new(SOURCE, 0, "empty")),
    };
    if !KNOWN_VERSIONS.contains(&version) {
        return Err(ParseError::new(
            SOURCE,
            1,
            format!("version {version} not in known versions {KNOWN_VERSIONS:?}"),
        ));
    }
    let mut fields = Vec::with_capacity(9);
    for (i, line) in lines {
        let mut parts = line.split_ascii_whitespace();
        let Some(label) = parts.next() else { continue };
        let Some(suffix) = label.strip_prefix("cpu") else {
            continue;
        };
        let cpu = suffix
            .parse::<u32>()
            .map_err(|_| ParseError::new(SOURCE, i + 1, format!("`{label}` is not a cpu label")))?;
        numbers(SOURCE, i + 1, parts, &mut fields)?;
        if fields.len() < 9 {
            return Err(ParseError::new(
                SOURCE,
                i + 1,
                format!("{} fields, need 9", fields.len()),
            ));
        }
        out.push(CpuSched {
            cpu,
            running_ns: fields[6],
            waiting_ns: fields[7],
        });
    }
    if out.is_empty() {
        return Err(ParseError::new(SOURCE, 0, "no cpu lines"));
    }
    Ok(())
}
