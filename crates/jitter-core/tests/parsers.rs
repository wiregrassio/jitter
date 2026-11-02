//! Parser contract tests: hand-built fixtures in the kernel's documented formats, and on Linux
//! the running kernel's own files, so CI on each architecture checks the real layout.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    missing_docs,
    non_snake_case
)]
// Tests read fixtures from disk; the core under test still may not.
#![allow(clippy::disallowed_methods)]

use jitter_core::parse::{diskstats, meminfo, net_dev, pressure, proc_stat, scalar, schedstat};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn proc_stat__aggregate_first_then_each_cpu() {
    let mut out = Vec::new();
    proc_stat::parse(&fixture("proc_stat.txt"), &mut out).unwrap();
    assert_eq!(out.len(), 5);
    assert_eq!(out[0].cpu, None);
    assert_eq!(out[1].cpu, Some(0));
    // user 1393 + nice 280 + system 260 + irq 21 + softirq 0
    assert_eq!(out[1].busy, 1954);
    assert_eq!(out[1].idle, 924110 + 13);
    assert_eq!(out[1].steal, 4);
}

#[test]
fn proc_stat__refuses_a_file_without_the_aggregate_line() {
    let mut out = Vec::new();
    let err = proc_stat::parse("cpu0 1 2 3 4 5 6 7 8\n", &mut out).unwrap_err();
    assert!(err.reason.contains("aggregate"), "{err}");
}

#[test]
fn proc_stat__refuses_a_short_line() {
    let mut out = Vec::new();
    assert!(proc_stat::parse("cpu 1 2 3 4\n", &mut out).is_err());
}

#[test]
fn schedstat__reads_running_and_waiting_columns() {
    let mut out = Vec::new();
    schedstat::parse(&fixture("schedstat.txt"), &mut out).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].cpu, 0);
    assert_eq!(out[0].running_ns, 1271867461917);
    assert_eq!(out[0].waiting_ns, 47383591024);
    assert_eq!(out[1].cpu, 1);
}

#[test]
fn schedstat__refuses_an_unknown_version() {
    let mut out = Vec::new();
    let text = fixture("schedstat.txt").replacen("version 15", "version 99", 1);
    let err = schedstat::parse(&text, &mut out).unwrap_err();
    assert!(err.reason.contains("99"), "{err}");
}

#[test]
fn meminfo__converts_kib_to_bytes() {
    let m = meminfo::parse_meminfo(&fixture("meminfo.txt")).unwrap();
    assert_eq!(m.total_bytes, 16127632 * 1024);
    assert_eq!(m.available_bytes, 14386448 * 1024);
    assert_eq!(m.swap_free_bytes, 2097148 * 1024);
}

#[test]
fn meminfo__refuses_a_kernel_without_memavailable() {
    let text = fixture("meminfo.txt").replace("MemAvailable", "MemAvail");
    let err = meminfo::parse_meminfo(&text).unwrap_err();
    assert!(err.reason.contains("MemAvailable"), "{err}");
}

#[test]
fn vmstat__reads_one_key() {
    assert_eq!(
        meminfo::parse_vmstat(&fixture("vmstat.txt"), "pgmajfault").unwrap(),
        4127
    );
    assert!(meminfo::parse_vmstat(&fixture("vmstat.txt"), "absent").is_err());
}

#[test]
fn pressure__reads_some_and_full_totals() {
    let p = pressure::parse(&fixture("pressure_memory.txt")).unwrap();
    assert_eq!(p.some_us, 1864508);
    assert_eq!(p.full_us, Some(902211));
    let cpu = pressure::parse(&fixture("pressure_cpu_old.txt")).unwrap();
    assert_eq!(cpu.full_us, None);
}

#[test]
fn net_dev__handles_no_space_after_the_colon() {
    let mut out = Vec::new();
    net_dev::parse(&fixture("net_dev.txt"), &mut out).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].name, "ens5");
    assert_eq!(out[1].rx_bytes, 4212877393);
    assert_eq!(out[1].rx_dropped, 17);
    assert_eq!(out[1].tx_bytes, 318822044);
    assert_eq!(out[1].tx_errors, 2);
}

#[test]
fn diskstats__reads_old_and_new_layouts() {
    let mut out = Vec::new();
    diskstats::parse(&fixture("diskstats.txt"), &mut out).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].name, "nvme0n1");
    assert_eq!(out[0].read_bytes, 3168934 * 512);
    assert_eq!(out[0].written_bytes, 12810218 * 512);
    assert_eq!(out[0].busy_ms, 210340);
    assert_eq!(out[2].name, "loop0");
}

#[test]
fn scalar__parses_temperature_and_statm() {
    assert_eq!(scalar::parse_millicelsius("temp", "45500\n").unwrap(), 45.5);
    assert!(scalar::parse_millicelsius("temp", "hot\n").is_err());
    assert_eq!(
        scalar::parse_statm_resident_pages("1024 300 200 10 0 400 0\n").unwrap(),
        300
    );
}

/// The running kernel's files, on Linux only. A parser that passes fixtures but fails here has
/// been written against the documentation instead of the kernel.
#[cfg(target_os = "linux")]
mod live {
    use super::*;

    fn read(path: &str) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn live__proc_stat() {
        let mut out = Vec::new();
        proc_stat::parse(&read("/proc/stat"), &mut out).unwrap();
        assert!(out.len() >= 2);
    }

    #[test]
    fn live__schedstat() {
        let mut out = Vec::new();
        schedstat::parse(&read("/proc/schedstat"), &mut out).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn live__meminfo_and_vmstat() {
        meminfo::parse_meminfo(&read("/proc/meminfo")).unwrap();
        meminfo::parse_vmstat(&read("/proc/vmstat"), "pgmajfault").unwrap();
    }

    #[test]
    fn live__net_dev_and_diskstats() {
        let mut ifs = Vec::new();
        net_dev::parse(&read("/proc/net/dev"), &mut ifs).unwrap();
        assert!(ifs.iter().any(|i| i.name == "lo"));
        let mut disks = Vec::new();
        diskstats::parse(&read("/proc/diskstats"), &mut disks).unwrap();
    }

    #[test]
    fn live__pressure_when_present() {
        // PSI is a kernel build option. Absent, the collector refuses to start; this test only
        // checks the parser against a kernel that has it.
        for r in ["cpu", "memory", "io"] {
            let path = format!("/proc/pressure/{r}");
            if let Ok(text) = std::fs::read_to_string(&path) {
                pressure::parse(&text).unwrap();
            }
        }
    }
}
