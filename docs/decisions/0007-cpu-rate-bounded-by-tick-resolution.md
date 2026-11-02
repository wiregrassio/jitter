# ADR 0007: The CPU sample rate is bounded by tick resolution

Status: Accepted
Date: 2026-11-02

## Context

`/proc/stat` reports CPU time in USER_HZ ticks, 100 per second. A sample spanning `n` ticks
resolves utilization to `100 / n` percentage points. Earlier generations sampled CPU at 100 Hz,
one tick per sample, so every per-CPU sample read 0 % or 100 % and the window's percentiles were
mostly quantization noise.

`/proc/schedstat` reports per-CPU run time and run-queue wait in nanoseconds. It was proposed as
the fix. Its counters advance only when a task switches out, so a CPU running one task without
switching (exactly the isolated-core case) reads idle until the task yields: nanosecond units,
context-switch resolution.

## Decision

- The `cpu` collector refuses any rate whose sample spans fewer than 10 ticks: 10 Hz at
  USER_HZ = 100. USER_HZ is read with `sysconf`, not assumed.
- The `sched` collector reports schedstat's run time and run-queue wait as their own streams,
  named for what they are. It is not presented as a finer `cpu`.
- Which source misleads when is measured, not argued (the comparison in
  `docs/architecture/footprint.md`).

## Alternatives rejected

- **Allow any rate and document the limit.** Rejected: a config that silently produces noise is
  the failure mode principle 3 exists to prevent.
- **Replace `cpu` with `sched`.** Rejected: each source misleads under a different load, so
  neither replaces the other.
- **Interpolate or smooth tick data.** Rejected: invents resolution the source does not have.

## Consequences

- CPU utilization distributions have at most ten samples per one-second window by default.
  `count` in each record states it.
- Run-queue wait is a first-class stream. It is the most direct per-CPU measure of scheduling
  contention Jitter has, and pairs with its own wake-up lateness.
