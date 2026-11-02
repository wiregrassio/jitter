//! A fake procfs and sysfs in a temp directory, and a config pointing at it.
#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A temp directory removed on drop.
pub struct Tree {
    pub root: PathBuf,
}

impl Tree {
    pub fn new() -> Tree {
        let root = std::env::temp_dir().join(format!(
            "jitter-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Tree { root }
    }

    pub fn proc(&self) -> PathBuf {
        self.root.join("proc")
    }

    pub fn sys(&self) -> PathBuf {
        self.root.join("sys")
    }

    /// Write `rel` under the root, creating parents. Rewrites in place, so an open handle sees
    /// the new contents on its next read from offset zero, as with a real kernel file.
    pub fn write(&self, rel: &str, text: &str) {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    pub fn stat(&self, cpu0: [u64; 8], cpu1: [u64; 8]) {
        let line = |label: &str, v: [u64; 8]| {
            format!(
                "{label} {} {} {} {} {} {} {} {} 0 0\n",
                v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]
            )
        };
        let mut all = [0u64; 8];
        for i in 0..8 {
            all[i] = cpu0[i] + cpu1[i];
        }
        let text = format!(
            "{}{}{}intr 1 2 3\nctxt 100\n",
            line("cpu ", all),
            line("cpu0", cpu0),
            line("cpu1", cpu1)
        );
        self.write("proc/stat", &text);
    }

    pub fn schedstat(&self, running: [u64; 2], waiting: [u64; 2]) {
        let mut text = String::from("version 15\ntimestamp 1\n");
        for cpu in 0..2 {
            text.push_str(&format!(
                "cpu{cpu} 0 0 1 1 1 1 {} {} 1\ndomain0 3 1 1 1\n",
                running[cpu], waiting[cpu]
            ));
        }
        self.write("proc/schedstat", &text);
    }

    pub fn memory(&self, available_kib: u64, majfault: u64) {
        self.write(
            "proc/meminfo",
            &format!(
                "MemTotal: 1000000 kB\nMemFree: 1 kB\nMemAvailable: {available_kib} kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n"
            ),
        );
        self.write(
            "proc/vmstat",
            &format!("pgfault 10\npgmajfault {majfault}\n"),
        );
    }

    pub fn pressure(&self, some_us: u64, full_us: u64) {
        for r in ["cpu", "memory", "io"] {
            self.write(
                &format!("proc/pressure/{r}"),
                &format!(
                    "some avg10=0.00 avg60=0.00 avg300=0.00 total={some_us}\nfull avg10=0.00 avg60=0.00 avg300=0.00 total={full_us}\n"
                ),
            );
        }
    }

    pub fn net(&self, rx: u64, tx: u64) {
        self.write(
            "proc/1/net/dev",
            &format!(
                "Inter-| Receive | Transmit\n face |bytes ...|bytes ...\n    lo: 1 1 0 0 0 0 0 0 1 1 0 0 0 0 0 0\n  eth0:{rx} 10 1 2 0 0 0 0 {tx} 10 3 4 0 0 0 0\n"
            ),
        );
    }

    pub fn diskstats(&self, read_sectors: u64, busy_ms: u64) {
        self.write(
            "proc/diskstats",
            &format!(" 259 0 vda 10 0 {read_sectors} 5 20 0 40 9 2 {busy_ms} 30 0 0 0 0\n"),
        );
    }

    pub fn statm(&self) {
        self.write("proc/self/statm", "1000 250 100 10 0 300 0\n");
    }

    pub fn thermal_zone(&self, n: u32, kind: &str, millic: i64) {
        self.write(
            &format!("sys/class/thermal/thermal_zone{n}/type"),
            &format!("{kind}\n"),
        );
        self.write(
            &format!("sys/class/thermal/thermal_zone{n}/temp"),
            &format!("{millic}\n"),
        );
    }

    /// Every file the default config reads, at a plausible first state.
    pub fn populate(&self) {
        self.stat([10, 0, 0, 90, 0, 0, 0, 0], [10, 0, 0, 90, 0, 0, 0, 0]);
        self.schedstat([0, 0], [0, 0]);
        self.memory(500_000, 0);
        self.pressure(0, 0);
        self.net(0, 0);
        self.diskstats(0, 0);
        self.statm();
        self.thermal_zone(0, "cpu-thermal", 40_000);
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The example config with placeholders filled and paths pointed at `tree`. Collectors that
/// need Linux system calls are left as the example sets them; callers adjust.
pub fn config_text(tree: &Tree, sink: &str) -> String {
    let example = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/jitter.example.toml"),
    )
    .unwrap();
    let text = example
        .replace("SENTINEL_SET_HOST", "test-host")
        .replace("SENTINEL_SET_INTERFACE", "eth0")
        .replace("SENTINEL_SET_DEVICE", "vda")
        .replace("SENTINEL_SET_ORG", "test-org")
        .replace("SENTINEL_SET_BUCKET", "test-bucket")
        .replace("proc = \"/proc\"", &format!("proc = {:?}", tree.proc()))
        .replace("sys = \"/sys\"", &format!("sys = {:?}", tree.sys()));
    match sink {
        "stdout" => {
            let cut = text.find("\n[sink.influxdb]").unwrap() + 1;
            text[..cut].replace("kind = \"influxdb\"", "kind = \"stdout\"")
        }
        url => text.replace("http://127.0.0.1:8086", url),
    }
}
