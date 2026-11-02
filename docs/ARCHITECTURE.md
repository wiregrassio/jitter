# Architecture

The map. Per-mechanism detail is in [`architecture/`](architecture/); the law behind each choice
is in [`PHILOSOPHY.md`](PHILOSOPHY.md), and the alternatives rejected are in
[`decisions/`](decisions/).

## The pipeline

```
 /proc, /sys,          sampling thread                                sink thread
 adjtimex, statvfs   ┌──────────────────────────────────────────┐   ┌──────────────────┐
 ───────────────────►│ collectors ──► windows ──► serializer ───┼──►│ InfluxDB 2.x     │
                     │   sample()      push()      write_to()   │   │ HTTP write, or   │
                     │        ▲                                 │   │ stdout           │
                     │   driver: absolute deadlines, lateness   │   └──────────────────┘
                     └──────────────────────────────────────────┘    bounded queue,
                                                                      drop and count
```

1. **Collectors** each read one kernel interface on their own sample rate and push values into
   streams. See [`architecture/collectors.md`](architecture/collectors.md).
2. **Windows** are per-stream sample buffers, allocated once. At each window boundary every
   stream reduces to one record: a distribution, or a point value for counters and capacities.
   See [`architecture/record-schema.md`](architecture/record-schema.md).
3. **The driver** runs every collector on one thread against absolute deadlines and measures
   how late each deadline was served. See [`architecture/timing.md`](architecture/timing.md).
4. **The sink** takes one serialized window at a time through a bounded queue and writes it to
   the store. See [`architecture/sink.md`](architecture/sink.md).

## Crates

| Crate | Holds | May not |
|-------|-------|---------|
| `jitter-core` | Kernel text parsers, counter deltas, the window and its reduction, the record type and its serializer | Read a file, a clock, an environment variable, or the network (`crates/jitter-core/clippy.toml`) |
| `jitter` | Config, the collectors' I/O, the driver, the sink, the self report, the few system calls | Format line protocol or reduce a window itself |

The dependency runs one way: `jitter` depends on `jitter-core`. The core has no dependencies at
all, so it can be wrapped by another composition (a Python binding, a different daemon) without
taking the daemon's choices with it.

## Threads

Two. The **sampling thread** runs the driver: it sleeps until the next deadline, samples, and at
window boundaries reduces, serializes, and offers the window to the sink without blocking. The
**sink thread** owns the store connection and is the only place that waits on I/O to another
process. No async runtime: two threads need no scheduler, and one fewer dependency is one fewer
thing in the footprint.

## State and failure

Jitter holds no durable state. Windows, previous counter reads, and the sink queue are in memory
and rebuilt from nothing on start. Every failure but one is fatal and exits with a named cause
and a distinct exit code; the restart supervisor brings it back clean. The one survivable failure
is the store, which costs counted, dropped windows. See
[`decisions/0005-source-failures-are-fatal.md`](decisions/0005-source-failures-are-fatal.md).

| Exit code | Meaning |
|-----------|---------|
| 64 | Bad command line |
| 69 | A source is missing or failed, at startup or while running |
| 74 | The stdout sink's reader went away |
| 78 | Invalid configuration, or a sink setting the environment does not satisfy |

## Per-mechanism documents

| Document | Mechanism |
|----------|-----------|
| [`architecture/collectors.md`](architecture/collectors.md) | The closed set, the collector interface, what each reads and why |
| [`architecture/timing.md`](architecture/timing.md) | Deadlines, windows, lateness, timer slack, and the CPU tick limit |
| [`architecture/record-schema.md`](architecture/record-schema.md) | Records, tags, naming, the reduction, the serializer |
| [`architecture/sink.md`](architecture/sink.md) | The bounded queue, confirmation, retry, drop counting |
| [`architecture/footprint.md`](architecture/footprint.md) | How the footprint and timing are measured, and on what |
