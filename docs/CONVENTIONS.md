# Conventions

Mechanical rules that keep [`PHILOSOPHY.md`](PHILOSOPHY.md) enforceable.

## Layout

```
crates/jitter-core/   pure: parse/, stats.rs, counter.rs, line.rs; fixtures under tests/
crates/jitter/        daemon: config, collectors/, driver, sink, http, self_report, sys
deploy/               the example config
bench/                footprint measurement harness
ci/                   CI scripts
docs/                 philosophy, architecture, decisions
```

One collector per file under `crates/jitter/src/collectors/`; its parser in
`crates/jitter-core/src/parse/`. A file that cannot be described in one sentence is split.

## Code

- Every `unsafe` block is a single libc call in `crates/jitter/src/sys.rs`, with a `SAFETY`
  comment.
- `unwrap`, `expect`, and `panic` are denied outside tests by workspace lints. Errors are typed
  enums with `Display`, named for the failure.
- Overflow checks are on in every profile; release builds abort on panic.
- Formatting is `cargo fmt`; clippy runs with warnings denied. Both gate CI.
- Test names are `<unit>__<behavior>`, so a failure reads as a sentence.

## Naming

- The service name comes from the package name (`env!("CARGO_PKG_NAME")`). It is never a
  string literal, so a rename cannot leave a stale tag behind.
- Measurements are `<namespace>.<signal>_<unit>`: `system.*` for the host, `jitter.*` for Jitter.
  Cumulative counters end in `_total`.
- Tags are bounded by hardware or config. An unbounded identifier is never a tag.

## Configuration

Every key required, every value validated at load, no defaults (ADR 0004). A new key is added to
`deploy/jitter.example.toml` in the same change, with a comment saying what it bounds; the config
tests parse the example, so forgetting fails CI.

## Tests

- Parsers: fixtures in the kernel's documented format, plus live-kernel tests on Linux CI on
  both architectures.
- Collectors: a fake procfs in a temp directory, driven with chosen instants so rates are exact.
- Pipeline: the real driver and sink against a mock store, for writes that land and writes that
  are refused.
- A data-integrity check runs in release builds, never only behind `debug_assert`.

## Documentation

- State why, not what the code says.
- One concern per document; cite, do not copy.
- A performance number names its method and host, or the doc says it is not measured.
- No em or en dashes in prose, docs, comments, or commit messages.
