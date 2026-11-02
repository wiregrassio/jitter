# Timing

How the driver decides when to sample, how windows are cut, and what the lateness stream
measures. Code: `crates/jitter/src/driver.rs`.

## Deadlines on a grid

At startup the driver anchors to the next wall-clock second boundary. Collector `c` samples at
`anchor + k * period_c`; windows close at `anchor + m * window`. Deadlines are computed from the
anchor, never from the previous wake-up, so error does not accumulate: a late wake-up delays one
sample, not every sample after it. Sample rates must divide one second in whole nanoseconds,
which keeps every collector's grid aligned with the window boundaries.

The driver serves deadlines in order. At a tie the window closes first, so a sample belongs to
the window its deadline falls in, however late it is served. A collector has `sample_hz * seconds`
deadlines per window therefore never pushes more than that many samples, which is exactly the
buffer size allocated at startup. A push past capacity is a scheduling bug and crashes.

If the loop falls a whole period behind, it skips the missed deadlines, counts them in
`jitter.self.missed_deadlines_total`, and resumes on the grid. It does not burst to catch up:
a burst would sample the same instant several times and report it as several.

Windows close on the same wall-clock seconds on every host, so records from many hosts line up
without resampling. A record's timestamp is the wall clock at the moment the window closed.

## Lateness

For every sample, `lateness = time the sample was read - its deadline`. The distribution of that
value per window is `jitter.self.wakeup_lateness_us`. It includes:

- **Timer slack.** The kernel may defer a timer wake-up by the thread's slack to batch wake-ups,
  50 us by default. `driver.timer_slack_ns` sets it on the sampling thread with
  `PR_SET_TIMERSLACK`. With the default, lateness is mostly the slack itself.
- **Scheduling delay.** Time between the timer firing and the thread running: other runnable
  tasks, and on a VM, the hypervisor.
- **Jitter's own work.** Time spent serving an earlier deadline that delayed this one.

This is why the stream is the project's namesake: it is a direct, continuous measurement of
timing noise on the host, from the inside, at every rate the driver runs.

## Why the CPU collector has a speed limit

`/proc/stat` reports CPU time in USER_HZ ticks. Linux fixes USER_HZ at 100 on mainstream
architectures (Jitter reads it with `sysconf(_SC_CLK_TCK)` rather than assuming). A sample of
`n` ticks can resolve utilization only to `100 / n` percentage points:

| `cpu.sample_hz` | Ticks per sample | Resolution |
|----------------:|-----------------:|-----------:|
| 100 | 1 | 100 points: every sample reads 0 % or 100 % |
| 20 | 5 | 20 points |
| 10 | 10 | 10 points |
| 1 | 100 | 1 point |

Config refuses any rate below 10 ticks per sample (`MIN_CPU_TICKS_PER_SAMPLE` in
`crates/jitter/src/config.rs`). At the default window of one second, a 10 Hz CPU stream gives ten
samples whose distribution means something. Faster timing comes from the `sched` collector, with
its own caveat ([`collectors.md`](collectors.md)). See
[`../decisions/0007-cpu-rate-bounded-by-tick-resolution.md`](../decisions/0007-cpu-rate-bounded-by-tick-resolution.md).

## Shutdown

SIGTERM or SIGINT sets a flag the loop checks every iteration. The longest sleep is one sample
period, at most one second, so stop is observed within a second. The partial window is closed
and offered to the sink, the queue is closed, and the sink thread finishes what is queued, bounded
by `queue_windows * max_attempts * (timeout_ms + retry_delay_ms)`.
