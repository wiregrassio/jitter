# ADR 0003: One sink, owned by the daemon, bounded, drop and count

Status: Accepted
Date: 2026-11-02

## Context

The write path is the one place an external failure (the store restarting, slow, or gone) meets
Jitter. It must not block sampling, must not grow memory, and must not hide loss.

## Decision

- One sink thread owns the store connection. The sampling thread offers each serialized window
  to a bounded queue without blocking.
- A full queue drops the window and counts it. The sink confirms each write by status code;
  a failed attempt is counted and retried up to a configured limit, then the window is dropped
  and counted.
- The counters (written, failed, dropped) ship in Jitter's own records every window.
- A window is the unit of loss. Nothing is spooled to disk.

## Alternatives rejected

- **A write client per collector.** Rejected: health, retries, and drops scatter, and "is the
  write path healthy" has no single answer.
- **Blocking on a full queue.** Rejected: a slow store would stall sampling, turning a store
  problem into a timing defect on the host being observed.
- **An unbounded queue.** Rejected: memory grows for the length of the outage.
- **A disk spool.** Rejected: telemetry is the most droppable payload, and a spool trades a
  bounded gap for unbounded local state and a new corruption surface.

## Consequences

- An outage costs a counted, bounded gap, and Jitter keeps running. This is the one survivable
  failure in the design (ADR 0005), so it is also the one that needs an alert: on the absence of
  a host's `jitter.self.*` records, and on any increase in dropped windows.
