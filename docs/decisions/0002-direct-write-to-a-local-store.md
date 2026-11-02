# ADR 0002: Write directly to a local store; InfluxDB 2.x or stdout

Status: Accepted
Date: 2026-11-02

## Context

Reduced records (ADR 0001) amount to roughly a hundred lines a second per host. A local
time-series store ingests thousands. Every component a hub would add between producer and store
(routing, rate limiting, buffering, a shared-memory bus) presumes a scarcity that is absent here,
and each is one more failure point on the path that must not fail silently.

An earlier redesign proposal put a zero-copy shared-memory bus and a separate aggregation agent
between the collector and the store, with the collector shipping high-frequency samples for the
agent to reduce. That contradicts ADR 0001 (it moves reduction off the source) and reintroduces
the hub this decision removes.

## Decision

- Jitter writes line protocol directly to **InfluxDB 2.x** over its HTTP write API, from one sink
  (ADR 0003). 2.x rather than 3.x Core: the latter's free edition limits queries to recent data,
  which defeats retention.
- The one alternative target is **stdout**, line protocol per window. It covers every other
  consumer (a file, a pipe into an agent that speaks line protocol) without a second network
  client.
- The HTTP client is Jitter's own: plain HTTP/1.1, one POST per connection, status checked. No
  TLS: the store is on this host or its private network.

## Alternatives rejected

- **A routing hub, broker, or shared-memory bus on the write path.** Rejected for the headroom
  reason above. A local zero-copy consumer of live records could sit *beside* the direct write
  later without contradicting this decision; it would not replace it.
- **A pluggable store interface.** Rejected: no second store is planned, and an abstraction for
  an unneeded swap costs vague docs and a query-side lowest common denominator. A different store
  would be a new decision and one new sink, not a revived abstraction.
- **A general HTTP library.** Rejected: its dependency tree (URL parsing, IDNA, TLS) was larger
  than the rest of Jitter combined, for one POST to localhost.
- **A disk spool for outages.** Rejected: see ADR 0003.

## Consequences

- The deployment is Jitter plus a store. Nothing in between to run, scale, or watch.
- Remote export, replication, and retention tiers are downstream of the store, by someone's
  explicit decision.
