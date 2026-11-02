//! Structured logs on stderr, one logfmt line per event: `level=warn event=window_dropped
//! queue=8`. Events are named so a reader can count them; prose goes in a `detail` field.

use std::fmt::Display;
use std::io::Write;

/// Severity.
#[derive(Debug, Clone, Copy)]
pub enum Level {
    /// Normal lifecycle: started, stopped.
    Info,
    /// Data was lost but Jitter is running as designed: a dropped window, a failed write.
    Warn,
    /// Jitter is about to exit nonzero.
    Error,
}

/// Emit one event.
pub fn event(level: Level, name: &str, fields: &[(&str, &dyn Display)]) {
    let level = match level {
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    };
    let mut line = format!("level={level} event={name}");
    for (key, value) in fields {
        let value = value.to_string();
        if value.is_empty() || value.contains([' ', '"', '=']) {
            line.push_str(&format!(" {key}={value:?}"));
        } else {
            line.push_str(&format!(" {key}={value}"));
        }
    }
    line.push('\n');
    // stderr is unbuffered; one write per line keeps lines whole across threads. A failed write
    // to stderr has nowhere to be reported.
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}
