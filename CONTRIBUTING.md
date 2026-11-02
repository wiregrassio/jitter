# Contributing

The rules below are gates, not guidelines. A change that fails one is not merged until it passes,
or until the principle it violates in [`docs/PHILOSOPHY.md`](docs/PHILOSOPHY.md) is changed
first, with an argument. Read that and [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md) first.

CI is the mechanical half: format, clippy with warnings denied, every test, a release build, and
a smoke run on the runner's real kernel, on x86_64 and arm64, plus a check at the minimum
supported Rust version. The judgment half is below.

## Gates

1. **The footprint is stated.** A change on the sampling or write path states its cost in
   numbers: wake-ups, allocations, syscalls per sample, or a `bench/run.sh` comparison. "It is
   only a little more" is not a number.
2. **Statistics, not samples.** Nothing ships raw samples. A new statistic recombines across
   windows (ADR 0001).
3. **A sample means what it says.** A new stream states its source's resolution, and a config
   that would sample below it is refused, not documented (ADR 0007).
4. **The collector set stays closed.** New signal extends an existing collector. A new collector
   is a new decision record arguing a new class of signal.
5. **Fail loud.** No fallback defaults, no catch-and-continue, no degraded mode. Config is
   validated at load. Loss is counted, never silent.
6. **The core stays pure.** Parsing and reduction live in `jitter-core`, take their inputs as
   arguments, and ship with fixture tests in the same change.
7. **One serializer.** Records are formatted only by `Record::write_to`.

## Pull requests

- One purpose per change.
- Name the gate a change comes near and how it stays inside it.
- Update the affected doc in the same change. A doc that describes behavior the code does not
  have is a defect.
- No em or en dashes in prose, comments, or commit messages.

## Scope

Out of scope by decision, and a design discussion (a decision record) before any code: a path
off the local network on Jitter's own initiative, a hub between Jitter and the store, a collector
plugin interface, an application metrics SDK, and log parsing.
