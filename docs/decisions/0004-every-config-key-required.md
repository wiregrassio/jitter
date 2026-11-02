# ADR 0004: Every configuration key is required

Status: Accepted
Date: 2026-11-02

## Context

The first generation fell back to plausible values when settings were missing: a device
identifier of "unknown", a default database name and user. A misconfigured host then looked
healthy and wrote mislabeled rows. Config fields that were parsed but never read lingered so old
files would still load.

## Decision

- Every key is required, including in disabled collector tables. Unknown keys are refused.
- Every value is range-checked at load, and cross-checked where values interact (sink kind and
  table, CPU rate and USER_HZ, rates that divide a second).
- Example values are `SENTINEL_...` and refused, so an unedited example cannot run.
- Secrets never appear in the file: the InfluxDB token is read from the environment variable the
  file names, and its absence fails at startup.
- `jitter check <config>` runs the whole validation and opens every source, then exits.

## Alternatives rejected

- **Defaults for "obvious" values.** Rejected: an obvious default is the dangerous kind, because
  it is plausible enough that nobody checks it.
- **Warn and continue on unknown keys.** Rejected: a typo becomes an ignored setting.

## Consequences

- Config files are longer than they would be with defaults. The example is complete and
  commented, so a new file starts from it.
- A config change is a restart. Nothing reloads at runtime; with no durable state, a restart
  costs at most one window.
