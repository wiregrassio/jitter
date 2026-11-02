# Jitter

Always-on Linux telemetry in Rust. Jitter samples the kernel at up to 1 kHz, reduces each
one-second window to a fixed distribution (count, min, p5, p50, p95, max, mean), and writes those
records straight to a local time-series store. It reports its own footprint and timing in the
same stream as the host's, so the claim "it costs almost nothing to run" is checked every second
instead of asserted once.

It reads only what Linux exposes in `/proc` and `/sys`, plus `adjtimex` and `statvfs`, so it runs
on any Linux. The reference hardware is AWS Graviton (arm64); development and CI run on x86_64
and arm64.

> **Status: early.** The collectors, driver, sink, and test suite are built and green in CI on
> both architectures. The Graviton footprint and timing measurements are not yet taken; the
> table below says so until they are.

## What it measures

A closed set of eight collectors, each reading one kernel interface:

| Collector | Source | Reports |
|-----------|--------|---------|
| `cpu` | `/proc/stat` | Utilization and steal per CPU and in aggregate |
| `sched` | `/proc/schedstat` | Per-CPU run time and run-queue wait, from nanosecond counters |
| `memory` | `/proc/meminfo`, `/proc/vmstat` | Available memory, swap, major fault rate |
| `pressure` | `/proc/pressure/*` | Share of wall time stalled on CPU, memory, I/O (PSI) |
| `network` | `/proc/1/net/dev` | Per-interface throughput; error and drop counters |
| `disk` | `/proc/diskstats`, `statvfs` | Per-device IOPS, throughput, busy time, queue depth; mount capacity |
| `thermal` | `/sys/class/thermal` | Temperature per zone, selected by zone type |
| `timesync` | `adjtimex(2)` | Clock offset, frequency correction, error bound, sync state |

And one about itself:

| Stream | Meaning |
|--------|---------|
| `jitter.self.wakeup_lateness_us` | How late each sampling deadline was served: timer slack, scheduling delay, and Jitter's own work |
| `jitter.self.cpu_percent` | Jitter's CPU time over the window, percent of one CPU |
| `jitter.self.resident_bytes` | Resident memory |
| `jitter.self.missed_deadlines_total` | Deadlines skipped because the loop fell a whole period behind |
| `jitter.sink.*_total` | Windows written, attempts failed, windows dropped |

Wake-up lateness is the stream the project is named for. On a quiet bare-metal core it is the
floor set by the kernel's timer path; on a VM it carries the hypervisor; under load it shows the
scheduler. It is the same measurement in all three places, so the comparison is direct.

## One finding that shaped the design

`/proc/stat` counts CPU time in USER_HZ ticks, 100 per second on every mainstream Linux build.
A 100 Hz CPU sample spans one tick per CPU, so its utilization can only read 0 % or 100 %, and a
"p95 of 100 Hz CPU utilization" is mostly quantization. Earlier generations of this collector
sampled CPU that way. Jitter refuses at startup any `cpu` rate whose sample would span fewer than
10 ticks (10 Hz at USER_HZ = 100), and the error message says why.

For finer timing it reads `/proc/schedstat`, whose counters are in nanoseconds. Nanosecond
units are not nanosecond resolution, though: those counters advance when a task switches out,
so a CPU running one task without a switch reads idle until the task yields. The two sources
are reported side by side, not merged, and their disagreement under different loads is one of
the measurements below.

## Footprint and timing

The headline claim is a bounded footprint. Until it is measured, it is a design target, not a
result:

| Platform | Jitter CPU | RSS | Wake-up lateness p50 / p95 / max | IPC | Status |
|----------|-----------:|----:|---------------------------------:|----:|--------|
| Graviton c7g VM | | | | | not yet measured |
| Graviton c7g.metal, isolated core, SCHED_FIFO | | | | | not yet measured |
| x86_64 lab host | | | | | not yet measured |

Method, in [`docs/architecture/footprint.md`](docs/architecture/footprint.md): `perf stat` with
hardware counters (cycles, instructions, cache references and misses) attached to the running
daemon, Arm's topdown level-1 metrics on Neoverse, and Jitter's own `self` streams over the same
interval. [`bench/run.sh`](bench/run.sh) records one run with the host facts that make it
comparable. Burstable instances (t4g) are excluded: their shared cores make timing numbers
meaningless.

What bounds the footprint by construction, before any measurement:

- Each stream's window is a buffer allocated once at startup, sized from the validated sample
  rate. Memory does not grow with uptime.
- Kernel files are opened once and re-read from offset zero, so a sample costs a seek and a read,
  not an open and a close.
- One sampling thread on absolute deadlines and one sink thread. No async runtime.
- The sink queue is bounded. A slow or absent store drops whole windows and counts them; it never
  blocks sampling or grows memory.

## Run it

Linux only. Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml).

```sh
cargo build --release
cp deploy/jitter.example.toml jitter.toml   # fill every SENTINEL value
target/release/jitter check jitter.toml     # validate config, open every source, exit
target/release/jitter run jitter.toml
```

Every config key is required and there are no defaults: a missing or out-of-range value is a
named startup failure. Records go to InfluxDB 2.x over its HTTP write API, or to stdout as line
protocol (`sink.kind = "stdout"`) for piping anywhere else. The InfluxDB token is read from the
environment variable the config names, never from the file.

Records look like this:

```
system.cpu.utilization_percent,service=jitter,host=lab-1,cpu=all count=10i,min=3.1,p5=3.1,p50=4.6,p95=9.2,max=9.2,mean=5.0 1762099201000000000
jitter.self.wakeup_lateness_us,service=jitter,host=lab-1 count=151i,min=2.1,p5=2.6,p50=4.0,p95=11.8,max=61.3,mean=5.2 1762099201000000000
```

(Illustrative values, not measurements.)

## How it is built

Two crates. `jitter-core` is pure: kernel text parsers, the window reduction, and the one
line-protocol serializer, with a lint configuration that fails the build if it touches a file,
a clock, or the network. `jitter` is the daemon: it reads the kernel, keeps time, and owns the
sink. Every parser is tested against fixtures in the kernel's documented format, and on Linux CI
against the runner's live kernel on both architectures.

| Path | Contents |
|------|----------|
| [`docs/PHILOSOPHY.md`](docs/PHILOSOPHY.md) | The design law, argued |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | The pipeline, timing, the sink, the failure model |
| [`docs/architecture/`](docs/architecture/) | One file per mechanism |
| [`docs/decisions/`](docs/decisions/) | Architecture decision records |
| [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md) | Code, naming, and documentation conventions |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | The contribution gates |

## Lineage

Jitter is the third generation of a collector built twice before. The first sampled at 100 Hz
and wrote through a message stream to a relational time-series store; the second reduced at the
source and wrote directly to InfluxDB. Jitter keeps what held (statistics instead of samples, a
direct local write, required config, fatal source failures, a pure core) and is the first to
measure its own cost. It shares no code with either.

## License

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
