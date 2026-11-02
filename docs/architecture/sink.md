# Sink

The output stage. Code: `crates/jitter/src/sink.rs` and `http.rs`.

## Shape

The sampling thread serializes each window into one batch and offers it to a bounded queue
(`sink.queue_windows`) with a non-blocking send. A full queue drops the window and increments
`jitter.sink.dropped_windows_total`; the first drop after a success logs one line, so an outage
is one log line, not one per second. Sampling never waits for the sink.

The sink thread owns the store connection. For each batch it POSTs to InfluxDB 2.x
`/api/v2/write?org=<org>&bucket=<bucket>&precision=ns`. A window counts as written only when
the store answers 2xx. A failed attempt increments `jitter.sink.failed_writes_total`, waits
`retry_delay_ms`, and retries, up to `max_attempts`; then the window is dropped and counted.
While the sink retries, the queue fills, and further windows drop at the queue: the bound on
memory holds through any outage.

`sink.kind = "stdout"` writes each window's lines to stdout instead, for piping into any other
consumer. If stdout's reader goes away, Jitter exits (code 74): nobody will see another record,
and running on would be the "up and writing nothing" failure.

## Why it is shaped this way

- **One sink, owned by the daemon.** Health, drops, and retries are answered in one place.
  Per-collector write clients would scatter them. See
  [`../decisions/0003-one-bounded-sink.md`](../decisions/0003-one-bounded-sink.md).
- **No disk spool.** Telemetry is the most droppable payload. A spool trades a bounded gap for
  unbounded local state and a corruption surface.
- **A small HTTP client of its own.** The store is local and speaks plain HTTP/1.1. The client is
  one POST per connection with `Connection: close`, under two hundred lines on `std::net`, instead
  of a general HTTP library that would multiply the dependency tree for features never used. See
  [`../decisions/0002-direct-write-to-a-local-store.md`](../decisions/0002-direct-write-to-a-local-store.md).
- **Plain HTTP only.** `https://` and URLs with paths or credentials are refused at startup. The
  store is on this host or its private network; TLS to a remote store is the downstream's job.

## Watching it

The sink is the one place Jitter survives an external failure, so it is the one place that needs
watching. Its counters are in Jitter's own records, written through the same sink: while the
store is down they cannot arrive, and when it returns the counters show what was lost. Alert on
the absence of `jitter.self.*` records for a host, and on any increase of
`jitter.sink.dropped_windows_total`.
