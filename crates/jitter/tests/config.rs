//! Config contract: the example parses once its placeholders are filled, and every rule that
//! refuses a config refuses it by name.
#![allow(clippy::unwrap_used, clippy::panic, missing_docs, non_snake_case)]

mod common;

use common::{Tree, config_text};
use jitter::config::{Config, ConfigError, Kernel};

const KERNEL: Kernel = Kernel { user_hz: 100 };

fn refused(text: &str) -> String {
    match Config::parse(text, KERNEL) {
        Ok(_) => panic!("config was accepted"),
        Err(ConfigError::Schema(e)) | Err(ConfigError::Invalid(e)) => e,
        Err(e) => panic!("unexpected {e}"),
    }
}

#[test]
fn config__example_with_placeholders_filled_is_valid() {
    let tree = Tree::new();
    Config::parse(&config_text(&tree, "http://127.0.0.1:8086"), KERNEL).unwrap();
    Config::parse(&config_text(&tree, "stdout"), KERNEL).unwrap();
}

#[test]
fn config__example_as_shipped_is_refused_for_its_sentinels() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../deploy/jitter.example.toml"
    ))
    .unwrap();
    assert!(refused(&text).contains("identity.host"));
}

#[test]
fn config__a_missing_key_is_refused() {
    let tree = Tree::new();
    let text = config_text(&tree, "stdout").replace("timer_slack_ns = 1000\n", "");
    assert!(refused(&text).contains("timer_slack_ns"));
}

#[test]
fn config__an_unknown_key_is_refused() {
    let tree = Tree::new();
    let text = config_text(&tree, "stdout").replace("[window]\n", "[window]\nsecond = 1\n");
    assert!(refused(&text).contains("second"));
}

#[test]
fn config__cpu_faster_than_ten_ticks_per_sample_is_refused() {
    let tree = Tree::new();
    let text = config_text(&tree, "stdout").replacen("sample_hz = 10", "sample_hz = 20", 1);
    let e = refused(&text);
    assert!(
        e.contains("collectors.cpu.sample_hz = 20") && e.contains("maximum is 10"),
        "{e}"
    );
}

#[test]
fn config__a_rate_that_does_not_divide_a_second_is_refused() {
    let tree = Tree::new();
    let text = config_text(&tree, "stdout").replace("sample_hz = 100", "sample_hz = 3");
    assert!(refused(&text).contains("does not divide"));
}

#[test]
fn config__sink_table_must_match_kind() {
    let tree = Tree::new();
    let influx_without_table =
        config_text(&tree, "stdout").replace("kind = \"stdout\"", "kind = \"influxdb\"");
    assert!(refused(&influx_without_table).contains("[sink.influxdb]"));
    let stdout_with_table = config_text(&tree, "http://127.0.0.1:8086")
        .replace("kind = \"influxdb\"", "kind = \"stdout\"");
    assert!(refused(&stdout_with_table).contains("remove one"));
}

#[test]
fn config__https_and_paths_in_the_store_url_are_refused() {
    let tree = Tree::new();
    for url in ["https://127.0.0.1:8086", "http://127.0.0.1:8086/api"] {
        assert!(refused(&config_text(&tree, url)).contains("url"), "{url}");
    }
}
