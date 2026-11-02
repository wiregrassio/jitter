//! Serializer contract: field identity and order are locked, escaping is reversible, and every
//! float reads back as the same float.
#![allow(clippy::unwrap_used, missing_docs, non_snake_case)]

use jitter_core::line::{DISTRIBUTION_FIELDS, FieldValue, Payload, Record};
use jitter_core::stats::summarize;

fn line(record: &Record) -> String {
    let mut out = String::new();
    record.write_to(&mut out);
    out
}

#[test]
fn distribution__field_names_and_order_are_fixed() {
    assert_eq!(
        DISTRIBUTION_FIELDS,
        ["count", "min", "p5", "p50", "p95", "max", "mean"]
    );
    let mut samples = vec![3.0, 1.0, 2.0];
    let record = Record {
        measurement: "system.cpu.utilization_percent",
        tags: vec![("service", "jitter".into()), ("cpu", "all".into())],
        payload: Payload::Distribution(summarize(&mut samples).unwrap()),
        timestamp_ns: 1_700_000_000_000_000_000,
    };
    assert_eq!(
        line(&record),
        "system.cpu.utilization_percent,service=jitter,cpu=all \
         count=3i,min=1.0,p5=1.0,p50=2.0,p95=3.0,max=3.0,mean=2.0 1700000000000000000\n"
    );
}

#[test]
fn point__integer_and_float_suffixes() {
    let int = Record {
        measurement: "system.network.rx_errors",
        tags: vec![],
        payload: Payload::Point(FieldValue::Int(7)),
        timestamp_ns: 1,
    };
    assert_eq!(line(&int), "system.network.rx_errors value=7i 1\n");
    let float = Record {
        payload: Payload::Point(FieldValue::Float(7.0)),
        ..int
    };
    assert_eq!(line(&float), "system.network.rx_errors value=7.0 1\n");
}

#[test]
fn tags__separators_are_escaped() {
    let hostile = "a b,c=d";
    let record = Record {
        measurement: "m",
        tags: vec![("host", hostile.into())],
        payload: Payload::Point(FieldValue::Int(1)),
        timestamp_ns: 1,
    };
    assert_eq!(line(&record), "m,host=a\\ b\\,c\\=d value=1i 1\n");
}

#[test]
fn floats__read_back_exactly() {
    for v in [0.1, 1e-300, 123456789.12345679, -0.0, f64::MAX] {
        let record = Record {
            measurement: "m",
            tags: vec![],
            payload: Payload::Point(FieldValue::Float(v)),
            timestamp_ns: 1,
        };
        let text = line(&record);
        let value = text
            .split(' ')
            .nth(1)
            .unwrap()
            .strip_prefix("value=")
            .unwrap();
        assert_eq!(
            value.parse::<f64>().unwrap().to_bits(),
            v.to_bits(),
            "{text}"
        );
    }
}
