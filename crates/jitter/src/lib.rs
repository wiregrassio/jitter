//! The Jitter daemon: reads the kernel, keeps time, owns the sink. Pure logic lives in
//! `jitter-core`; this crate is the part that touches the system.

pub mod collectors;
pub mod config;
pub mod driver;
pub mod http;
pub mod log;
pub mod self_report;
pub mod sink;
pub mod source;
pub mod sys;

/// Bad command line.
pub const EXIT_USAGE: u8 = 64;
/// A source is missing or failed (`EX_UNAVAILABLE`).
pub const EXIT_SOURCE: u8 = 69;
/// The sink failed in a way that ends collection (`EX_IOERR`).
pub const EXIT_SINK: i32 = 74;
/// Invalid configuration (`EX_CONFIG`).
pub const EXIT_CONFIG: u8 = 78;
