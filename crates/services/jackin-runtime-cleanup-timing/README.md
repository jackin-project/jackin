# jackin-runtime-cleanup-timing

Cleanup timing
guard and
failure
reporting.

## What this crate owns

- Timing
  (`timing`):
  `CleanupTiming`,
  `cleanup_timing` —
  scoped diagnostics
  timing guard for a
  named cleanup
  phase.
- Reporting
  (`timing`):
  `cleanup_failure` —
  telemetry error
  record for a failed
  cleanup step.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`timing.rs`](src/timing.rs) | timing guard + failure reporting | hub `cleanup` suite |

## Public API

`timing::CleanupTiming`,
`timing::cleanup_timing`,
`timing::cleanup_failure`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-timing
cargo clippy -p jackin-runtime-cleanup-timing --all-targets -- -D warnings
```
