# Footprint and timing measurement

How the headline claim is measured. Results go in the README table with the host facts from the
run; a number without its method and host is not published.

## What is measured

| Quantity | Source | Why this source |
|----------|--------|-----------------|
| CPU time | `perf stat` task-clock on the Jitter PID; `jitter.self.cpu_percent` | Two independent readings of the same thing; they must agree |
| Instructions, cycles, IPC | `perf stat -e cycles,instructions` | What the work costs on the core, independent of frequency |
| Cache behavior | `perf stat -e cache-references,cache-misses` | Whether Jitter's working set disturbs the workload's cache |
| Topdown level 1 | `perf stat -M TopdownL1` on Neoverse (Graviton) | Where Jitter's cycles go: frontend bound, backend bound, retiring, bad speculation |
| Resident memory | `jitter.self.resident_bytes` | Must be flat over hours; growth is a defect |
| Wake-up lateness | `jitter.self.wakeup_lateness_us` | The timing noise floor, per platform |
| Context switches, migrations | `perf stat` software events | The scheduler cost of two threads |

`bench/run.sh` runs Jitter with the stdout sink to a file, skips startup, attaches `perf stat`
for a fixed interval, and records kernel, CPU model, instance type, kernel command line, binary
hash, and pinning alongside the counters.

## Where

| Platform | Purpose |
|----------|---------|
| Graviton c7g or m7g VM | The normal deployment shape. Includes hypervisor noise in lateness. |
| Graviton c7g.metal, isolated core | The floor: `isolcpus` and `nohz_full` on the kernel command line, Jitter pinned with `taskset`, run under `chrt -f` (SCHED_FIFO). |
| x86_64 lab host | Cross-architecture comparison of the same binary's cost. |

Burstable instances (t4g, t3) are excluded: CPU credits and shared cores make timing numbers
describe the neighbor, not Jitter.

## Comparisons that matter

- **VM against metal**, same instance family, same config: how much of lateness is the
  hypervisor.
- **Timer slack 50 us against 1 us**: how much of lateness is the kernel's own batching.
- **`cpu` against `sched` utilization** under a steady compute load and under a bursty one:
  where the tick-based and switch-based sources agree and where each misleads.
- **Footprint against sample rate**: `sched` at 10, 100, and 1000 Hz. Cost should scale with
  wake-ups, and the slope is the per-sample cost.

## Status

Not yet run. The README table is empty until it is.
