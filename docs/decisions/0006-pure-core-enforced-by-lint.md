# ADR 0006: A pure core crate, enforced by lint

Status: Accepted
Date: 2026-11-02

## Context

Parsing and reduction must be deterministic and testable without hardware (principle 7). A
directory convention says that; nothing enforces it, and a single convenient `File::open` or
`Instant::now()` in a parser makes it untestable against fixtures.

## Decision

`jitter-core` holds every parser, the counter deltas, the window and its reduction, and the
record serializer. It has no dependencies. Its `clippy.toml` disallows `std::fs` reads, `Instant`
and `SystemTime` reads, environment reads, and `TcpStream`, and CI runs clippy with warnings
denied, so reaching for I/O in the core fails the build. Its tests may read fixture files; the
allowance is in the test files only.

## Alternatives rejected

- **One crate with module discipline.** Rejected: discipline is not an architecture.
- **Traits injected for every I/O.** Rejected: more machinery than the boundary needs. Parsers
  take `&str`, the reduction takes `&mut [f64]`, and time is a parameter.

## Consequences

- The core can be wrapped by another composition (a Python binding, a different daemon) without
  inheriting the daemon's threads or I/O.
- Every collector has two halves: a pure parser in the core, tested on fixtures, and a thin
  reader in the daemon, tested on a fake procfs and on CI against the live kernel.
