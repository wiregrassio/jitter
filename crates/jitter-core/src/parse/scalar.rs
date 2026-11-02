//! One-value files: thermal zone temperatures and `/proc/self/statm`.

use super::ParseError;

/// Parse a thermal zone `temp` file (millidegrees Celsius) into degrees Celsius.
pub fn parse_millicelsius(source: &'static str, text: &str) -> Result<f64, ParseError> {
    let t = text.trim();
    t.parse::<i64>()
        .map(|m| m as f64 / 1000.0)
        .map_err(|_| ParseError::new(source, 1, format!("`{t}` is not an integer")))
}

/// Parse `/proc/self/statm` and return resident pages (the second field).
pub fn parse_statm_resident_pages(text: &str) -> Result<u64, ParseError> {
    text.split_ascii_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| ParseError::new("/proc/self/statm", 1, "no integer resident field"))
}
