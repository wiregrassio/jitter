//! The pure core of Jitter: kernel text parsers, the window reduction, and the line-protocol
//! serializer. Nothing in this crate reads a file, a clock, or the network; `clippy.toml` here
//! makes reaching for one a lint failure. Every function is testable against captured text.

pub mod counter;
pub mod line;
pub mod parse;
pub mod stats;
