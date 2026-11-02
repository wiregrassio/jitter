# ADR 0005: Collector source failures are fatal

Status: Accepted
Date: 2026-11-02

## Context

A collector source can fail to open, fail to read, change shape, or change identity (a CPU going
offline, an interface disappearing, a counter going backwards). The second generation first
argued for backoff on such failures, treating them as external and transient, then reversed:
on hosts with stable kernel interfaces, a runtime source failure is not transient, and partial
collection is worse than none, because a missing stream looks like "nothing changed" unless
someone checks which collectors are running.

## Decision

Every collector failure exits the process with a named cause (exit code 69). The supervisor
(systemd, a container runtime) restarts it; a persistent failure becomes a restart loop, which
is visible and alertable. The sink is the one exception (ADR 0003).

## Alternatives rejected

- **Backoff and retry per collector.** Rejected: it adds a reachable state (running, partly
  collecting) that has to be tested to be trusted, and it hides exactly the failures operators
  need to see.
- **Disable the failing collector and continue.** Rejected for the same reason, worse.

## Consequences

- A crash-only process needs a supervisor. Without one, a single read failure ends collection.
- Liveness has an external signal: records stop arriving. A future integration with a lease-based
  coordination service (a process clock whose lapse is the failure signal) could give consumers
  that signal without reading the store; it would sit beside this rule, not replace it.
