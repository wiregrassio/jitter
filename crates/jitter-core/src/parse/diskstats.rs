//! `/proc/diskstats`: per-block-device I/O counters.

use super::{ParseError, numbers};

const SOURCE: &str = "/proc/diskstats";

/// Sectors in `/proc/diskstats` are always 512 bytes, whatever the device's block size.
pub const SECTOR_BYTES: u64 = 512;

/// Cumulative counters for one block device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    /// Device name, such as `nvme0n1`.
    pub name: String,
    /// Reads completed.
    pub reads: u64,
    /// Bytes read.
    pub read_bytes: u64,
    /// Writes completed.
    pub writes: u64,
    /// Bytes written.
    pub written_bytes: u64,
    /// I/Os currently in flight. A gauge, not a counter.
    pub in_flight: u64,
    /// Milliseconds the device had at least one I/O in flight.
    pub busy_ms: u64,
}

/// Parse every device line into `out`, in file order.
pub fn parse(text: &str, out: &mut Vec<Disk>) -> Result<(), ParseError> {
    out.clear();
    let mut fields = Vec::with_capacity(20);
    for (i, line) in text.lines().enumerate() {
        let mut parts = line.split_ascii_whitespace();
        let (Some(_major), Some(_minor), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        numbers(SOURCE, i + 1, parts, &mut fields)?;
        // reads merged sectors ms writes merged sectors ms in_flight io_ms weighted_ms, then
        // optional discard (4.18) and flush (5.5) groups.
        if fields.len() < 11 {
            return Err(ParseError::new(
                SOURCE,
                i + 1,
                format!("{} counters, need at least 11", fields.len()),
            ));
        }
        out.push(Disk {
            name: name.to_string(),
            reads: fields[0],
            read_bytes: fields[2] * SECTOR_BYTES,
            writes: fields[4],
            written_bytes: fields[6] * SECTOR_BYTES,
            in_flight: fields[8],
            busy_ms: fields[9],
        });
    }
    Ok(())
}
