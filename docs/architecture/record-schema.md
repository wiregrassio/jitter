# Record schema

One record type, one serializer. Code: `crates/jitter-core/src/line.rs` and `stats.rs`.

## Wire form

InfluxDB line protocol, one record per line:

```
<measurement>,service=jitter,host=<host>[,<stream tags>] <fields> <timestamp_ns>
```

- **Measurement**: `<namespace>.<signal>_<unit>`. `system.*` for the host, `jitter.*` for Jitter
  itself. The unit is in the name (`_percent`, `_bytes`, `_bytes_per_second`, `_seconds`,
  `_us`, `_celsius`, `_total` for cumulative counts), so a reader never guesses it.
- **Tags**: `service` (from the package name at build time, so it cannot drift), `host` (from
  config, never inferred: the hostname differs inside a container), then the stream's own
  dimensions: `cpu`, `interface`, `device`, `mount`, `zone`, `resource`, `kind`. All are
  low-cardinality and bounded by the host's hardware.
- **Timestamp**: nanoseconds since the Unix epoch, UTC, at window close.

## Two payloads

| Payload | Fields | For |
|---------|--------|-----|
| Distribution | `count` (integer), `min`, `p5`, `p50`, `p95`, `max`, `mean` | Every sampled stream |
| Point | `value` (integer or float) | Cumulative counters, capacities, states |

Percentiles are nearest-rank: the smallest sample with at least `p` percent of samples at or
below it. With ten samples, p5 is the minimum and p95 is the maximum; the record says `count` so
a reader knows how much the tails mean. A window with no samples emits no record.

## Recombining windows

A reader asking about an hour recombines 3600 one-second records:

| Statistic | Across windows |
|-----------|----------------|
| `count` | Sum. Exact. |
| `min`, `max` | Min of mins, max of maxes. Exact. |
| `mean` | Count-weighted mean of means. Exact. |
| `p5`, `p50`, `p95` | Count-weighted percentile of percentiles. **An estimate**, and labeled as one by any consumer. |

A statistic that does not recombine does not belong in the record. See
[`../decisions/0001-reduce-at-the-source.md`](../decisions/0001-reduce-at-the-source.md).

## One serializer

`Record::write_to` is the only code that formats line protocol. Field names and order are a
constant (`DISTRIBUTION_FIELDS`) locked by a test, floats are written in Rust's shortest
round-trip form and read back bit-identical in a test, and tag separators are escaped. A second
serializer anywhere is a contribution gate failure: duplicated field order is the classic silent
corruption, one writer updated and another not.
