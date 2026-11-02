//! `/proc/pressure/{cpu,memory,io}`: pressure stall information, cumulative microseconds.

use super::ParseError;

const SOURCE: &str = "/proc/pressure";

/// Cumulative stall time for one resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressure {
    /// Microseconds during which at least one task was stalled on the resource.
    pub some_us: u64,
    /// Microseconds during which every non-idle task was stalled. Older kernels omit the
    /// `full` line for cpu.
    pub full_us: Option<u64>,
}

/// Parse one pressure file. Only the `total=` counters are read; the kernel's `avgN` figures
/// are its own exponential averages and do not recombine.
pub fn parse(text: &str) -> Result<Pressure, ParseError> {
    let mut some = None;
    let mut full = None;
    for (i, line) in text.lines().enumerate() {
        let mut parts = line.split_ascii_whitespace();
        let slot = match parts.next() {
            Some("some") => &mut some,
            Some("full") => &mut full,
            _ => continue,
        };
        let total = parts
            .find_map(|p| p.strip_prefix("total="))
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| ParseError::new(SOURCE, i + 1, "no integer `total=`"))?;
        *slot = Some(total);
    }
    Ok(Pressure {
        some_us: some.ok_or_else(|| ParseError::new(SOURCE, 0, "missing `some` line"))?,
        full_us: full,
    })
}
