//! The one record type and the one serializer. Every record Jitter writes passes through
//! [`Record::write_to`]; nothing else formats line protocol.
//!
//! Wire form: `measurement,tag=v,tag=v field=v,field=v timestamp_ns`. Tags are low-cardinality
//! dimensions; fields carry values. Tag order is the order the caller built them in, and the
//! daemon always builds `service`, then `host`, then the stream's own tags.

use crate::stats::Summary;
use std::fmt::Write as _;

/// One field value. Integers carry the `i` suffix on the wire; floats never do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldValue {
    /// A float field.
    Float(f64),
    /// A signed integer field.
    Int(i64),
}

/// The value payload of a record: a window summary or a single point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Payload {
    /// A reduced window: `count,min,p5,p50,p95,max,mean`.
    Distribution(Summary),
    /// One reading: `value`. Used for cumulative counters, capacities, and states.
    Point(FieldValue),
}

/// One line of line protocol.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Measurement name, `<namespace>.<signal>_<unit>`.
    pub measurement: &'static str,
    /// Tags, in wire order.
    pub tags: Vec<(&'static str, String)>,
    /// The fields.
    pub payload: Payload,
    /// Nanoseconds since the Unix epoch, UTC.
    pub timestamp_ns: i64,
}

/// The distribution field names, in wire order. Fixed by test.
pub const DISTRIBUTION_FIELDS: [&str; 7] = ["count", "min", "p5", "p50", "p95", "max", "mean"];

impl Record {
    /// Append this record and a trailing newline to `out`.
    pub fn write_to(&self, out: &mut String) {
        escape_into(out, self.measurement, Escape::Measurement);
        for (key, value) in &self.tags {
            out.push(',');
            escape_into(out, key, Escape::Tag);
            out.push('=');
            escape_into(out, value, Escape::Tag);
        }
        out.push(' ');
        match self.payload {
            Payload::Distribution(s) => {
                let values = [
                    FieldValue::Int(i64::try_from(s.count).unwrap_or(i64::MAX)),
                    FieldValue::Float(s.min),
                    FieldValue::Float(s.p5),
                    FieldValue::Float(s.p50),
                    FieldValue::Float(s.p95),
                    FieldValue::Float(s.max),
                    FieldValue::Float(s.mean),
                ];
                for (i, (name, value)) in DISTRIBUTION_FIELDS.iter().zip(values).enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(name);
                    out.push('=');
                    write_value(out, value);
                }
            }
            Payload::Point(value) => {
                out.push_str("value=");
                write_value(out, value);
            }
        }
        // Writing into a String cannot fail.
        let _ = writeln!(out, " {}", self.timestamp_ns);
    }
}

fn write_value(out: &mut String, value: FieldValue) {
    // Writing into a String cannot fail. `{:?}` on f64 always round-trips and always carries a
    // decimal point or exponent, so a float never reads back as an integer.
    let _ = match value {
        FieldValue::Float(v) => write!(out, "{v:?}"),
        FieldValue::Int(v) => write!(out, "{v}i"),
    };
}

#[derive(Clone, Copy)]
enum Escape {
    Measurement,
    Tag,
}

fn escape_into(out: &mut String, s: &str, kind: Escape) {
    for c in s.chars() {
        let escape = match kind {
            Escape::Measurement => matches!(c, ',' | ' '),
            Escape::Tag => matches!(c, ',' | ' ' | '='),
        };
        if escape {
            out.push('\\');
        }
        // Line protocol has no escape for a newline; it would split the record. Tag values come
        // from validated config and kernel names, so replacement is a last line of defense.
        out.push(if c == '\n' { ' ' } else { c });
    }
}
