//! `/proc/meminfo` and `/proc/vmstat`: memory capacity and fault counters.

use super::ParseError;

const MEMINFO: &str = "/proc/meminfo";
const VMSTAT: &str = "/proc/vmstat";

/// The `/proc/meminfo` values Jitter reports, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemInfo {
    /// `MemTotal`.
    pub total_bytes: u64,
    /// `MemAvailable`: the kernel's estimate of memory a new workload could use without swapping.
    pub available_bytes: u64,
    /// `SwapTotal`.
    pub swap_total_bytes: u64,
    /// `SwapFree`.
    pub swap_free_bytes: u64,
}

/// Parse `/proc/meminfo`. Every key is required; a kernel without `MemAvailable` (before 3.14)
/// is refused rather than estimated.
pub fn parse_meminfo(text: &str) -> Result<MemInfo, ParseError> {
    let mut found: [Option<u64>; 4] = [None; 4];
    const KEYS: [&str; 4] = ["MemTotal", "MemAvailable", "SwapTotal", "SwapFree"];
    for (i, line) in text.lines().enumerate() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(slot) = KEYS.iter().position(|k| *k == key) else {
            continue;
        };
        let mut parts = rest.split_ascii_whitespace();
        let value = parts
            .next()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| {
                ParseError::new(MEMINFO, i + 1, format!("`{key}` has no integer value"))
            })?;
        if parts.next() != Some("kB") {
            return Err(ParseError::new(
                MEMINFO,
                i + 1,
                format!("`{key}` is not in kB"),
            ));
        }
        found[slot] = Some(value * 1024);
    }
    let get = |slot: usize| {
        found[slot].ok_or_else(|| ParseError::new(MEMINFO, 0, format!("missing `{}`", KEYS[slot])))
    };
    Ok(MemInfo {
        total_bytes: get(0)?,
        available_bytes: get(1)?,
        swap_total_bytes: get(2)?,
        swap_free_bytes: get(3)?,
    })
}

/// Read one key from `/proc/vmstat`, such as `pgmajfault`.
pub fn parse_vmstat(text: &str, key: &str) -> Result<u64, ParseError> {
    for (i, line) in text.lines().enumerate() {
        let mut parts = line.split_ascii_whitespace();
        if parts.next() == Some(key) {
            return parts
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| {
                    ParseError::new(VMSTAT, i + 1, format!("`{key}` has no integer value"))
                });
        }
    }
    Err(ParseError::new(VMSTAT, 0, format!("missing `{key}`")))
}
