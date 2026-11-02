//! `/proc/net/dev`: per-interface byte, packet, error, and drop counters.

use super::{ParseError, numbers};

const SOURCE: &str = "/proc/net/dev";

/// Cumulative counters for one interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    /// Interface name, such as `ens5`.
    pub name: String,
    /// Bytes received.
    pub rx_bytes: u64,
    /// Receive errors.
    pub rx_errors: u64,
    /// Received packets dropped.
    pub rx_dropped: u64,
    /// Bytes transmitted.
    pub tx_bytes: u64,
    /// Transmit errors.
    pub tx_errors: u64,
    /// Transmitted packets dropped.
    pub tx_dropped: u64,
}

/// Parse every interface line into `out`, in file order.
pub fn parse(text: &str, out: &mut Vec<Interface>) -> Result<(), ParseError> {
    out.clear();
    let mut fields = Vec::with_capacity(16);
    // Two header lines, then `name: 16 counters`. The kernel pads names but may print no space
    // after the colon, so split on the colon, not on whitespace.
    for (i, line) in text.lines().enumerate().skip(2) {
        let Some((name, rest)) = line.split_once(':') else {
            return Err(ParseError::new(
                SOURCE,
                i + 1,
                "no `:` after interface name",
            ));
        };
        numbers(SOURCE, i + 1, rest.split_ascii_whitespace(), &mut fields)?;
        if fields.len() != 16 {
            return Err(ParseError::new(
                SOURCE,
                i + 1,
                format!("{} counters, need 16", fields.len()),
            ));
        }
        out.push(Interface {
            name: name.trim().to_string(),
            rx_bytes: fields[0],
            rx_errors: fields[2],
            rx_dropped: fields[3],
            tx_bytes: fields[8],
            tx_errors: fields[10],
            tx_dropped: fields[11],
        });
    }
    Ok(())
}
