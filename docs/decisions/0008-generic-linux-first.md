# ADR 0008: Generic Linux first; scope cuts carried forward

Status: Accepted
Date: 2026-11-02

## Context

The second generation targeted one embedded GPU platform, with its GPU and thermal paths
behind an explicit platform binding. Jitter's reference hardware is a general-purpose arm64
server (AWS Graviton), with x86_64 alongside. Several surfaces were built and then cut in earlier
generations, each for a reason that still holds.

## Decision

- **Generic Linux first.** Every collector reads interfaces every Linux kernel exposes. Nothing
  assumes a board. Platform-specific signal (an embedded GPU's load and frequency, a board's
  thermal map) arrives later as an optional binding: paths and constants in one place per
  platform, captured fixtures for CI, and a named startup failure on a platform nobody
  enumerated, never guessed paths.
- **No application metrics SDK.** It couples a bounded hardware collector to an unbounded
  application surface; applications already have a direct path to the store.
- **No interactive terminal UI.** A collector without a store is pointless on a headless host,
  and a second read path doubles the system reads.
- **No container accounting yet.** When it comes, it reads cgroup v2 files directly, not a
  container runtime's API: the runtime's stats path has returned silent zeros and per-container
  serial fetches that missed their sample rate.
- **No write gate.** Every window emits every stream. Suppressing unchanged streams saves volume
  the store does not need saved, and costs the simplest liveness signal there is: presence.

## Consequences

- Jitter runs unmodified on any Linux with procfs and sysfs: cloud VMs, bare metal, lab machines,
  embedded boards for the generic signals.
- Each cut is reversible by a new decision record that answers the reason recorded here.
