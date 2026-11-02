# ADR 0001: Reduce at the source and ship statistics

Status: Accepted
Date: 2026-11-02

## Context

Collectors sample at up to 1 kHz, per CPU, per interface, per device. Something has to turn that
into what a reader uses. Shipping raw samples keeps every point and costs bandwidth, storage, and
reader effort in proportion to the sample rate; reducing per window bounds the volume and gives
up the raw points.

Both earlier generations of this collector reduced each window to order statistics and dropped
higher moments. The second made recombination explicit: min, max, count, and mean recombine
exactly; percentiles only approximately.

## Decision

Each stream reduces per window to `count, min, p5, p50, p95, max, mean` (nearest-rank
percentiles) before leaving the process. Counters, capacities, and states ship as one point
value. A statistic enters the record only if it recombines across windows.

## Alternatives rejected

- **Ship raw samples and reduce downstream** (for example in a log or metrics pipeline in front
  of the store). Rejected: the volume scales with the sample rate, contradicting the footprint
  principle and the direct write, and the downstream does work the producer could do with data
  already in hand.
- **Fixed-bucket histograms.** Rejected: bucket bounds are a configuration surface per signal,
  percentile reconstruction from buckets is lossy in its own way, and the target store does not
  merge them natively.
- **Mergeable sketches (t-digest, DDSketch).** Deferred, not rejected: they recombine better than
  per-window percentiles, but the store cannot query them natively. Revisit if long-range
  percentiles become the main read.
- **Skew and kurtosis.** Rejected: near meaningless once recombined, and nothing reads them.

## Consequences

- Volume is bounded by streams per window, independent of sample rate.
- Long-range percentiles are estimates, and say so wherever they are consumed.
- With few samples per window the tails are coarse (ten samples: p5 is the min); `count` rides
  every record so a reader can tell.
