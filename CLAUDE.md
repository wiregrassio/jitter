# Jitter

Always-on Linux telemetry collector in Rust: samples `/proc` and `/sys`, reduces each window to a
fixed distribution, writes line protocol directly to a local store, and reports its own footprint
and timing. `README.md` is the overview; `docs/PHILOSOPHY.md` is the law every change answers to.

## Contracts

1. **Linux only at runtime.** `crates/jitter/src/main.rs::main` exits 69 on any other OS; the
   crates build elsewhere so the pure core and fake-procfs tests run anywhere.
2. **The core is pure.** `jitter-core` reads no file, clock, environment, or socket:
   `crates/jitter-core/clippy.toml`, enforced by `cargo clippy -- -D warnings` in CI.
3. **Every config key is required; there are no defaults.** `crates/jitter/src/config.rs`
   (`deny_unknown_fields`, no `serde(default)`, `Config::validate`); the example config is parsed
   by `crates/jitter/tests/config.rs`.
4. **The CPU rate is capped by tick resolution.** `MIN_CPU_TICKS_PER_SAMPLE` in
   `crates/jitter/src/config.rs`; ADR 0007.
5. **Sample rates divide one second; deadlines are an absolute grid.**
   `crates/jitter/src/config.rs::Config::validate`, `crates/jitter/src/driver.rs::Driver::run`.
6. **A window never grows.** `crates/jitter-core/src/stats.rs::Window::push` refuses past
   capacity; the driver's tie rule (close before sample) keeps it from being reached.
7. **One serializer, fixed field order.** `crates/jitter-core/src/line.rs::Record::write_to`,
   `DISTRIBUTION_FIELDS`; locked by `crates/jitter-core/tests/serializer.rs`.
8. **No reading is not a zero.** First counter reads and empty windows emit nothing:
   `crates/jitter-core/src/counter.rs::Last::advance`, `stats.rs::Window::close`.
9. **Collector failures are fatal; the sink drops and counts.** `CollectError` in
   `crates/jitter/src/collectors/mod.rs`; `crates/jitter/src/sink.rs::SinkHandle::offer`; ADRs
   0003, 0005.
10. **`service` comes from the package name.** `crates/jitter/src/driver.rs::Driver::new`.
11. **All `unsafe` is in `crates/jitter/src/sys.rs`**, one libc call per block.
12. **No em or en dashes** in docs, comments, or commit messages.

## Map

| Path | Purpose |
|------|---------|
| `crates/jitter-core/` | Parsers (`src/parse/`), window reduction, counters, serializer; fixtures in `tests/fixtures/` |
| `crates/jitter/` | Daemon: config, collectors, driver, sink, HTTP client, self report, syscalls; tests on a fake procfs |
| `deploy/jitter.example.toml` | The complete, commented config; every value a reader must set is `SENTINEL_...` |
| `bench/run.sh` | One footprint measurement run: `perf stat`, topdown, host facts |
| `ci/smoke.sh` | CI smoke run of the release binary against the runner's kernel |
| `docs/` | `PHILOSOPHY.md`, `ARCHITECTURE.md`, `architecture/` (one file per mechanism), `decisions/` (ADRs, immutable once accepted; a reversal is a new ADR), `CONVENTIONS.md` |
| `.github/workflows/ci.yml` | The mechanical contribution gate, x86_64 and arm64 |
