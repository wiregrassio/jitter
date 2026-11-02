//! Collector contract tests against a fake procfs and sysfs. Each test drives `sample` with
//! chosen instants, so rates are exact.
#![allow(clippy::unwrap_used, clippy::panic, missing_docs, non_snake_case)]

mod common;

use common::Tree;
use jitter::collectors::cpu::Cpu;
use jitter::collectors::disk::DiskCollector;
use jitter::collectors::memory::Memory;
use jitter::collectors::network::Network;
use jitter::collectors::pressure::PressureCollector;
use jitter::collectors::sched::Sched;
use jitter::collectors::thermal::ThermalCollector;
use jitter::collectors::{CloseContext, CollectError, Collector};
use jitter_core::line::{FieldValue, Payload, Record};
use std::time::{Duration, Instant};

fn ctx() -> CloseContext {
    CloseContext {
        service: "jitter".into(),
        host: "h".into(),
        timestamp_ns: 1,
    }
}

fn close(c: &mut dyn Collector) -> Vec<Record> {
    let mut out = Vec::new();
    c.close(&ctx(), &mut out);
    out
}

fn find<'a>(records: &'a [Record], measurement: &str, tag: (&str, &str)) -> &'a Record {
    records
        .iter()
        .find(|r| {
            r.measurement == measurement && r.tags.iter().any(|(k, v)| *k == tag.0 && v == tag.1)
        })
        .unwrap_or_else(|| panic!("no {measurement} {tag:?} in {records:#?}"))
}

fn mean(r: &Record) -> f64 {
    match r.payload {
        Payload::Distribution(s) => s.mean,
        Payload::Point(_) => panic!("{} is a point", r.measurement),
    }
}

#[test]
fn cpu__utilization_and_steal_from_tick_deltas() {
    let tree = Tree::new();
    tree.stat([10, 0, 0, 90, 0, 0, 0, 0], [0, 0, 0, 100, 0, 0, 0, 0]);
    let mut cpu = Cpu::open(&tree.proc(), 10, 1).unwrap();
    let t0 = Instant::now();
    cpu.sample(t0).unwrap();
    // cpu0: 3 user + 2 system busy, 4 idle, 1 steal over 10 ticks. cpu1: all idle.
    tree.stat([13, 0, 2, 94, 0, 0, 0, 1], [0, 0, 0, 110, 0, 0, 0, 0]);
    cpu.sample(t0 + Duration::from_millis(100)).unwrap();
    let records = close(&mut cpu);
    assert_eq!(
        mean(find(
            &records,
            "system.cpu.utilization_percent",
            ("cpu", "0")
        )),
        50.0
    );
    assert_eq!(
        mean(find(&records, "system.cpu.steal_percent", ("cpu", "0"))),
        10.0
    );
    assert_eq!(
        mean(find(
            &records,
            "system.cpu.utilization_percent",
            ("cpu", "1")
        )),
        0.0
    );
    assert_eq!(
        mean(find(
            &records,
            "system.cpu.utilization_percent",
            ("cpu", "all")
        )),
        25.0
    );
    assert_eq!(records[0].tags[0], ("service", "jitter".to_string()));
    assert_eq!(records[0].tags[1], ("host", "h".to_string()));
}

#[test]
fn cpu__first_sample_alone_emits_nothing() {
    let tree = Tree::new();
    tree.populate();
    let mut cpu = Cpu::open(&tree.proc(), 10, 1).unwrap();
    cpu.sample(Instant::now()).unwrap();
    assert!(close(&mut cpu).is_empty());
}

#[test]
fn cpu__a_changed_cpu_set_is_fatal() {
    let tree = Tree::new();
    tree.populate();
    let mut cpu = Cpu::open(&tree.proc(), 10, 1).unwrap();
    tree.write("proc/stat", "cpu 1 0 0 1 0 0 0 0\ncpu0 1 0 0 1 0 0 0 0\n");
    assert!(matches!(
        cpu.sample(Instant::now()),
        Err(CollectError::Missing(_))
    ));
}

#[test]
fn cpu__a_backwards_counter_is_fatal() {
    let tree = Tree::new();
    tree.stat([10, 0, 0, 90, 0, 0, 0, 0], [10, 0, 0, 90, 0, 0, 0, 0]);
    let mut cpu = Cpu::open(&tree.proc(), 10, 1).unwrap();
    cpu.sample(Instant::now()).unwrap();
    tree.stat([5, 0, 0, 90, 0, 0, 0, 0], [10, 0, 0, 90, 0, 0, 0, 0]);
    assert!(matches!(
        cpu.sample(Instant::now()),
        Err(CollectError::Counter(..))
    ));
}

#[test]
fn sched__running_and_wait_over_measured_time() {
    let tree = Tree::new();
    tree.schedstat([0, 0], [0, 0]);
    let mut sched = Sched::open(&tree.proc(), 100, 1).unwrap();
    let t0 = Instant::now();
    sched.sample(t0).unwrap();
    // 10 ms elapsed: cpu0 ran 7.5 ms, and tasks waited 15 ms in total (two waiters).
    tree.schedstat([7_500_000, 0], [15_000_000, 0]);
    sched.sample(t0 + Duration::from_millis(10)).unwrap();
    let records = close(&mut sched);
    assert_eq!(
        mean(find(&records, "system.sched.running_percent", ("cpu", "0"))),
        75.0
    );
    assert_eq!(
        mean(find(
            &records,
            "system.sched.runqueue_wait_percent",
            ("cpu", "0")
        )),
        150.0
    );
}

#[test]
fn memory__available_and_fault_rate() {
    let tree = Tree::new();
    tree.memory(500_000, 100);
    let mut mem = Memory::open(&tree.proc(), 10, 1).unwrap();
    let t0 = Instant::now();
    mem.sample(t0).unwrap();
    tree.memory(400_000, 150);
    mem.sample(t0 + Duration::from_millis(500)).unwrap();
    let records = close(&mut mem);
    let available = records
        .iter()
        .find(|r| r.measurement == "system.memory.available_bytes")
        .unwrap();
    assert_eq!(mean(available), 450_000.0 * 1024.0);
    let faults = records
        .iter()
        .find(|r| r.measurement == "system.memory.major_faults_per_second")
        .unwrap();
    assert_eq!(mean(faults), 100.0);
}

#[test]
fn pressure__absent_psi_names_the_kernel_option() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.proc()).unwrap();
    match PressureCollector::open(&tree.proc(), 10, 1) {
        Err(CollectError::Missing(m)) => assert!(m.contains("CONFIG_PSI"), "{m}"),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("opened without PSI"),
    }
}

#[test]
fn pressure__stall_share_of_wall_time() {
    let tree = Tree::new();
    tree.pressure(0, 0);
    let mut p = PressureCollector::open(&tree.proc(), 10, 1).unwrap();
    let t0 = Instant::now();
    p.sample(t0).unwrap();
    tree.pressure(20_000, 5_000);
    p.sample(t0 + Duration::from_millis(100)).unwrap();
    let records = close(&mut p);
    let some = records
        .iter()
        .find(|r| {
            r.tags.contains(&("resource", "io".into())) && r.tags.contains(&("kind", "some".into()))
        })
        .unwrap();
    assert_eq!(mean(some), 20.0);
}

#[test]
fn network__rates_and_cumulative_counters() {
    let tree = Tree::new();
    tree.net(1_000, 0);
    let mut net = Network::open(&tree.proc(), &["eth0".into()], 10, 1).unwrap();
    let t0 = Instant::now();
    net.sample(t0).unwrap();
    tree.net(1_000 + 125_000, 50_000);
    net.sample(t0 + Duration::from_millis(250)).unwrap();
    let records = close(&mut net);
    assert_eq!(
        mean(find(
            &records,
            "system.network.rx_bytes_per_second",
            ("interface", "eth0")
        )),
        500_000.0
    );
    assert_eq!(
        mean(find(
            &records,
            "system.network.tx_bytes_per_second",
            ("interface", "eth0")
        )),
        200_000.0
    );
    let errors = find(
        &records,
        "system.network.tx_errors_total",
        ("interface", "eth0"),
    );
    assert_eq!(errors.payload, Payload::Point(FieldValue::Int(3)));
}

#[test]
fn network__a_missing_interface_lists_what_is_present() {
    let tree = Tree::new();
    tree.net(0, 0);
    match Network::open(&tree.proc(), &["ens5".into()], 10, 1) {
        Err(CollectError::Missing(m)) => assert!(m.contains("ens5") && m.contains("eth0"), "{m}"),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("opened a missing interface"),
    }
}

#[test]
fn disk__rates_busy_and_mount_capacity() {
    let tree = Tree::new();
    tree.diskstats(0, 0);
    let mut disk =
        DiskCollector::open(&tree.proc(), &["vda".into()], &["/".into()], 10, 1).unwrap();
    let t0 = Instant::now();
    disk.sample(t0).unwrap();
    tree.diskstats(2_048, 50);
    disk.sample(t0 + Duration::from_millis(100)).unwrap();
    let records = close(&mut disk);
    assert_eq!(
        mean(find(
            &records,
            "system.disk.read_bytes_per_second",
            ("device", "vda")
        )),
        2_048.0 * 512.0 * 10.0
    );
    assert_eq!(
        mean(find(
            &records,
            "system.disk.busy_percent",
            ("device", "vda")
        )),
        50.0
    );
    let total = find(&records, "system.filesystem.total_bytes", ("mount", "/"));
    assert!(matches!(total.payload, Payload::Point(FieldValue::Int(n)) if n > 0));
}

#[test]
fn thermal__zone_resolved_by_type() {
    let tree = Tree::new();
    tree.thermal_zone(0, "soc-thermal", 41_500);
    tree.thermal_zone(1, "cpu-thermal", 52_000);
    let mut t = ThermalCollector::open(&tree.sys(), &["cpu-thermal".into()], 10, 1).unwrap();
    t.sample(Instant::now()).unwrap();
    let records = close(&mut t);
    assert_eq!(
        mean(find(
            &records,
            "system.thermal.temperature_celsius",
            ("zone", "cpu-thermal")
        )),
        52.0
    );
}

#[test]
fn thermal__an_ambiguous_type_is_refused() {
    let tree = Tree::new();
    tree.thermal_zone(0, "cpu-thermal", 1);
    tree.thermal_zone(1, "cpu-thermal", 2);
    assert!(ThermalCollector::open(&tree.sys(), &["cpu-thermal".into()], 10, 1).is_err());
}

#[test]
fn window__more_samples_than_the_window_holds_is_fatal() {
    let tree = Tree::new();
    tree.populate();
    let mut t = ThermalCollector::open(&tree.sys(), &["cpu-thermal".into()], 2, 1).unwrap();
    t.sample(Instant::now()).unwrap();
    t.sample(Instant::now()).unwrap();
    assert!(matches!(
        t.sample(Instant::now()),
        Err(CollectError::Window(..))
    ));
}

/// The running kernel, on Linux only: the assumptions fixtures cannot check.
#[cfg(target_os = "linux")]
mod live {
    use super::*;
    use jitter::collectors::timesync::Timesync;
    use jitter::source::Source;

    #[test]
    fn live__procfs_regenerates_on_reread_from_offset_zero() {
        // Source keeps the file open and seeks back. If procfs served the first read's bytes
        // again, every rate would read zero; ctxt rises between reads on any live system.
        let mut stat = Source::open("/proc/stat").unwrap();
        let ctxt = |text: &str| -> u64 {
            text.lines()
                .find_map(|l| l.strip_prefix("ctxt "))
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        let first = ctxt(stat.read().unwrap());
        std::thread::sleep(Duration::from_millis(50));
        let second = ctxt(stat.read().unwrap());
        assert!(second > first, "ctxt {first} then {second}");
    }

    #[test]
    fn live__cpu_and_sched_against_the_running_kernel() {
        let mut cpu = Cpu::open(std::path::Path::new("/proc"), 10, 1).unwrap();
        let mut sched = Sched::open(std::path::Path::new("/proc"), 100, 1).unwrap();
        for _ in 0..3 {
            cpu.sample(Instant::now()).unwrap();
            sched.sample(Instant::now()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        let records = close(&mut cpu);
        assert!(!records.is_empty());
        assert!(!close(&mut sched).is_empty());
    }

    #[test]
    fn live__adjtimex_reads_the_clock_state() {
        let mut t = Timesync::open(1, 1).unwrap();
        t.sample(Instant::now()).unwrap();
        let records = close(&mut t);
        assert!(
            records
                .iter()
                .any(|r| r.measurement == "system.clock.max_error_seconds")
        );
    }
}
