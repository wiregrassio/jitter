# Collectors

The input stage: a closed set of collectors, each reading one kernel interface (principle 5 in
[`../PHILOSOPHY.md`](../PHILOSOPHY.md)). Code: `crates/jitter/src/collectors/`.

## The interface

```rust
trait Collector {
    fn name(&self) -> &'static str;
    fn period(&self) -> Duration;
    fn sample(&mut self, now: Instant) -> Result<(), CollectError>;
    fn close(&mut self, ctx: &CloseContext, out: &mut Vec<Record>);
}
```

`sample` reads the source once and pushes values into the collector's streams; `now` is taken
just before the read and is the instant the sample describes. `close` reduces every stream to a
record and starts a new window. A collector knows nothing about scheduling or transport; the
driver owns both ([`timing.md`](timing.md)).

Every collector is built from two stream shapes in `collectors/mod.rs`: `Dist`, reduced to a
distribution each window, and `Point`, which reports its latest value (cumulative counters,
capacities, states). Opening a collector opens and parses every source it will read, so anything
missing fails before the first sample.

## The set

| Collector | Reads | Streams | Notes |
|-----------|-------|---------|-------|
| `cpu` | `/proc/stat` | `system.cpu.utilization_percent`, `system.cpu.steal_percent`, per CPU and `cpu=all` | Ratios of tick deltas, so no clock is involved. Rate capped by tick resolution ([`timing.md`](timing.md)). |
| `sched` | `/proc/schedstat` | `system.sched.running_percent`, `system.sched.runqueue_wait_percent`, per CPU | Nanosecond counters that advance at context switches. Wait can exceed 100 %: two tasks waiting for a whole interval is 200 %. Refuses unknown schedstat versions. |
| `memory` | `/proc/meminfo`, `/proc/vmstat` | `system.memory.available_bytes`, `..._total_bytes`, `..._swap_*_bytes`, `system.memory.major_faults_per_second` | Requires `MemAvailable` (kernel 3.14+). |
| `pressure` | `/proc/pressure/{cpu,memory,io}` | `system.pressure.stall_percent`, tags `resource`, `kind=some/full` | Reads the `total=` counters, not the kernel's `avgN`, which are its own decaying averages and do not recombine. Needs `CONFIG_PSI`. |
| `network` | `/proc/1/net/dev` | `system.network.{rx,tx}_bytes_per_second`, `..._{errors,dropped}_total`, per interface | PID 1's view: the host's namespace when Jitter runs in a container with the host's procfs, without host networking. |
| `disk` | `/proc/diskstats`, `statvfs` | `system.disk.{read,write}_bytes_per_second`, `{reads,writes}_per_second`, `busy_percent`, `in_flight`; `system.filesystem.{total,available}_bytes` per mount | Diskstats sectors are always 512 bytes. `busy_percent` is the share of time with I/O in flight, not saturation: a parallel device can be 100 % busy and far from full. |
| `thermal` | `/sys/class/thermal/thermal_zone*/{type,temp}` | `system.thermal.temperature_celsius`, per zone type | Zones are chosen by `type`, because zone numbers follow probe order. A type matching zero or several zones is refused. Most cloud VMs have no zones. |
| `timesync` | `adjtimex(2)`, read only | `system.clock.offset_seconds`, `..._frequency_ppm`, `..._max_error_seconds`, `..._synchronized` | Reads the kernel's clock discipline, which every NTP or PTP client steers, so it works with any client. The error bound grows without limit once sync is lost, a better signal than stratum. |

The self report (`jitter.self.*`, `jitter.sink.*`) is not in the table because it is not a
collector: the driver owns it and feeds it lateness directly. See [`timing.md`](timing.md) and
[`footprint.md`](footprint.md).

## Identity is fixed at startup

The CPU set, interfaces, devices, and zones are resolved when a collector opens. A CPU going
offline, an interface disappearing, or a counter going backwards (a device replaced under the
same name) is a fatal error, because the stream's identity is no longer what the records claim.
See [`../decisions/0005-source-failures-are-fatal.md`](../decisions/0005-source-failures-are-fatal.md).

## Reading a kernel file

`source.rs` opens each file once and, per sample, seeks to offset zero and reads into a reused
buffer. procfs and sysfs regenerate contents on a read at offset zero, so this replaces an
`open` and `close` per sample with one `lseek`, and a steady-state read allocates nothing. A
live-kernel test on CI checks the regeneration on both architectures.

## Not here, yet

GPU and platform-specific thermal maps (an NVIDIA Jetson binding, for instance) and per-cgroup
container accounting are outside the generic set. Each would arrive as its own decision. See
[`../decisions/0008-generic-linux-first.md`](../decisions/0008-generic-linux-first.md).
