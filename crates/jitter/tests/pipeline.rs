//! End to end on a fake procfs: config, collectors, driver, sink, and a mock store that answers
//! like InfluxDB 2.x. Checks what lands and what is counted when the store refuses.
#![allow(clippy::unwrap_used, clippy::panic, missing_docs, non_snake_case)]

mod common;

use common::{Tree, config_text};
use jitter::collectors;
use jitter::config::{Config, Kernel};
use jitter::driver::{self, Driver};
use jitter::self_report::SelfReport;
use jitter::sink::SinkHandle;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Request path and body, per request received.
type Requests = Arc<Mutex<Vec<(String, String)>>>;

/// Accepts connections, records each request's path and body, answers with `status`.
fn mock_store(status: &'static str) -> (String, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" {
                    break;
                }
                if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let path = request_line.split(' ').nth(1).unwrap().to_string();
            log.lock()
                .unwrap()
                .push((path, String::from_utf8(body).unwrap()));
            let reply =
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    (url, seen)
}

/// Run the daemon's pieces for `run_for` against `url`, with timesync off (it needs Linux and is
/// covered by the live test) and thermal on (the fake tree has a zone).
fn run(tree: &Tree, url: &str, token_env: &str, run_for: Duration) -> Arc<jitter::sink::SinkStats> {
    let text = config_text(tree, url)
        .replace("JITTER_INFLUXDB_TOKEN", token_env)
        .replace("[collectors.timesync]\nenabled = true", "[collectors.timesync]\nenabled = false")
        .replace("[collectors.thermal]\n# Most cloud VMs expose no thermal zones. List zone `type` names to enable.\nenabled = false", "[collectors.thermal]\nenabled = true")
        .replace("zones = []", "zones = [\"cpu-thermal\"]");
    let config = Config::parse(&text, Kernel { user_hz: 100 }).unwrap();
    let built = collectors::build(&config).unwrap();
    let wakeups = driver::wakeups_per_window(&built, config.window.seconds);
    let mut sink = SinkHandle::start(&config.sink).unwrap();
    let report = SelfReport::open(&config.paths.proc, wakeups, sink.stats()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = Arc::clone(&stop);
    std::thread::spawn(move || {
        std::thread::sleep(run_for);
        stopper.store(true, Ordering::Relaxed);
    });
    Driver::new(
        built,
        Some(report),
        config.window.seconds,
        "test-host".into(),
        stop,
    )
    .run(&mut sink)
    .unwrap();
    let stats = sink.stats();
    sink.finish();
    stats
}

#[test]
fn pipeline__windows_land_in_the_store_with_identity_tags() {
    let tree = Tree::new();
    tree.populate();
    let (url, seen) = mock_store("204 No Content");
    // SAFETY: each test uses its own variable name, and nothing else reads it concurrently.
    unsafe { std::env::set_var("JITTER_TEST_TOKEN_OK", "test-token") };
    let stats = run(
        &tree,
        &url,
        "JITTER_TEST_TOKEN_OK",
        Duration::from_millis(2_500),
    );

    let seen = seen.lock().unwrap();
    assert!(seen.len() >= 2, "{} writes", seen.len());
    assert_eq!(
        stats.written.load(Ordering::Relaxed),
        u64::try_from(seen.len()).unwrap()
    );
    assert_eq!(stats.dropped.load(Ordering::Relaxed), 0);
    let (path, body) = &seen[seen.len() - 1];
    assert_eq!(
        path,
        "/api/v2/write?org=test-org&bucket=test-bucket&precision=ns"
    );
    for measurement in [
        "system.memory.available_bytes,service=jitter,host=test-host ",
        "system.thermal.temperature_celsius,service=jitter,host=test-host,zone=cpu-thermal ",
        "jitter.self.wakeup_lateness_us,service=jitter,host=test-host ",
        "jitter.self.resident_bytes,service=jitter,host=test-host ",
    ] {
        assert!(body.contains(measurement), "no `{measurement}` in:\n{body}");
    }
}

#[test]
fn pipeline__a_refusing_store_is_counted_not_hidden() {
    let tree = Tree::new();
    tree.populate();
    let (url, _) = mock_store("401 Unauthorized");
    // SAFETY: as above.
    unsafe { std::env::set_var("JITTER_TEST_TOKEN_BAD", "wrong-token") };
    let stats = run(
        &tree,
        &url,
        "JITTER_TEST_TOKEN_BAD",
        Duration::from_millis(1_500),
    );
    assert_eq!(stats.written.load(Ordering::Relaxed), 0);
    assert!(stats.dropped.load(Ordering::Relaxed) >= 1);
    assert!(stats.failed.load(Ordering::Relaxed) >= 3);
}
