//! Parsers for the kernel's text interfaces. Each takes the file's bytes as `&str` and returns
//! typed values; none reads a file. Fixture tests feed them text captured from real kernels.

pub mod diskstats;
pub mod meminfo;
pub mod net_dev;
pub mod pressure;
pub mod proc_stat;
pub mod scalar;
pub mod schedstat;

/// A kernel file did not have the shape its parser requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The interface being parsed, such as `/proc/stat`.
    pub source: &'static str,
    /// 1-based line number, or 0 when the problem is the file as a whole.
    pub line: usize,
    /// What was wrong.
    pub reason: String,
}

impl ParseError {
    pub(crate) fn new(source: &'static str, line: usize, reason: impl Into<String>) -> Self {
        ParseError {
            source,
            line,
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.source, self.reason)
        } else {
            write!(f, "{} line {}: {}", self.source, self.line, self.reason)
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse whitespace-separated unsigned integers, failing on the first that is not one.
pub(crate) fn numbers<'a>(
    source: &'static str,
    line: usize,
    fields: impl Iterator<Item = &'a str>,
    out: &mut Vec<u64>,
) -> Result<(), ParseError> {
    out.clear();
    for field in fields {
        let n = field.parse::<u64>().map_err(|_| {
            ParseError::new(
                source,
                line,
                format!("`{field}` is not an unsigned integer"),
            )
        })?;
        out.push(n);
    }
    Ok(())
}
