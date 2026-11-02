# Philosophy

The design law of Jitter. These are the constraints the system was derived from, not
preferences. A change that contradicts a principle changes the principle first, here, with an
argument. Each section argues its principle rather than asserting it.

## 1. Observation must not harm production

The system being observed is the one that matters. A monitor that steals CPU from the workload,
saturates a link with its own traffic, or grows in memory with uptime has inverted its purpose.
Breaking production to observe it is a defect, ranked with any other.

So the footprint is a hard requirement, met by construction:

- Each stream's window is allocated once, sized from the validated rate, and never grows.
- The write path drops under backpressure instead of queueing, so a slow store cannot make
  Jitter consume memory.
- Jitter holds no durable state, so it has nothing to accumulate.

And it is **measured, not assumed**. A footprint nobody measured is a hope. Jitter reports its
own CPU time, resident memory, and timing every window, in the same stream as the host's, and
the reference measurements with hardware counters are recorded in the README with their method.
A change that raises steady-state cost states the cost in numbers (see
[`../CONTRIBUTING.md`](../CONTRIBUTING.md)).

## 2. Statistics, not samples

A collector sampling at 100 Hz across 64 CPUs produces 6400 values a second. Shipping them raw
costs bandwidth and storage, and hands the reader the job of summarizing what the producer
already held.

A distribution says more for less. Each window reduces to count, min, p5, p50, p95, max, and
mean: one small record that carries the window's shape. So: **reduce at the source, ship the
distribution**.

A statistic belongs in the record only if it recombines across windows. Count, min, max, and the
count-weighted mean recombine exactly. Percentiles recombine only approximately, which is stated
wherever they are consumed. Higher moments are not computed: recombined, they mean almost
nothing, and no realistic query reads them.

## 3. A sample means what it says

A number in the store must mean what its name says, at the resolution it implies. This sounds
obvious and is the principle most easily broken without noticing:

- A utilization sampled faster than its source counter advances is quantization, not data.
  `/proc/stat` counts in 10 ms ticks; a 10 ms sample of it reads 0 % or 100 %. Jitter refuses
  rates that would do this rather than ship the noise.
- A source's units are not its resolution. `/proc/schedstat` is in nanoseconds but advances
  only at context switches. Jitter reports it beside the tick-based figure, named for what it
  is, instead of presenting it as a better version of the same number.
- No reading is not a zero. The first read of a counter has no rate; an interval with no ticks
  carries no information. Neither produces a sample, and a window with no samples produces no
  record.
- Rates divide by measured elapsed time, never by the nominal period, so a late wake-up does not
  become a spike.

## 4. Local-first, direct write

Records go from the collector straight to a store on the same host or private network. At
roughly a hundred records a second against a store that ingests thousands, every job a router,
broker, or shock absorber would do assumes a scarcity that does not exist. A bounded queue that
drops whole windows, and counts them, is the only backpressure device, and at local headroom it
engages only while the store restarts.

Nothing leaves the local network on Jitter's initiative. Replication, long-term tiering, and
cloud export are someone else's explicit decision, made downstream of the store.

## 5. Closed collector set

The collectors are a fixed set, each reading one kernel interface and tested against it. New
signal comes from extending a collector's output, not from a plugin surface. A closed set can be
reasoned about whole: every read path is known, the footprint covers an enumerable surface, and
every collector is held to the same contract. Adding a member takes a decision record arguing a
genuinely new class of signal.

## 6. Fail loud, restart fast

Any deviation from a known-good state is a crash with a named cause, not a degraded limp. The
worst failure is the silent one: a collector that stays up while writing nothing, or writing
wrong values, is found only when someone notices an absence.

- **Every config key is required.** A plausible default runs on a wrong value while looking
  healthy. Placeholders are obviously fake (`SENTINEL_...`) and refused.
- **Validate at startup.** Every source is opened and parsed before the first sample. A
  configured interface that does not exist, a kernel without PSI, an unknown schedstat version:
  each fails by name before collection starts.
- **Source failures are fatal.** The targets have stable kernel interfaces, so a failure at
  runtime is not transient, and a partly running collector set looks like "nothing changed".
  The restart supervisor (systemd, a container runtime) is part of the design; its restart loop
  is what an operator alerts on.
- **The one survivable failure is the store.** The sink retries a bounded number of times, then
  drops the window and counts it. Those counters are in Jitter's own records, so "up and writing
  nothing" is visible wherever the records are, and alerting on their absence catches the rest.

Fast restart is what makes this affordable. Jitter holds no durable state; a crash costs at most
one window, which the droppability of telemetry already permits.

## 7. The determinism boundary

Parsing and reduction are pure functions of their inputs: same text in, same values out, no
clock, no globals, no files. They live in `jitter-core`, whose lint configuration fails the
build if they reach for a file, a clock, or the network. Everything nondeterministic (when a
sample is taken, whether a write lands) lives in the daemon crate, at the edges.

This is what makes the closed set verifiable rather than merely enumerable: every parser is
tested against fixed text with no hardware in the loop, and on CI against the live kernel of each
architecture, so a parser written against documentation instead of the kernel fails there.

## How they compose

Statistics-not-samples (2) keeps the volume small. Small volume makes the direct local write (4)
and the bounded footprint (1) achievable without a hub. A closed set (5) and the determinism
boundary (7) keep that footprint and correctness provable. Meaning what it says (3) keeps the
small volume worth reading. Fail-loud (6) refuses the silent degradation that would let any of
the others stop being true unnoticed.
