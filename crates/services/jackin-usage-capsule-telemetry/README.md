# jackin-usage-capsule-telemetry

In-container telemetry lifecycle: OTLP export and
bounded startup for the Capsule daemon, plus
telemetry-level state and panic handling for the
multiplexer.

## What this crate owns

- Lifecycle (`telemetry`): Capsule identity
  claim, exporter install, bounded startup,
  and shutdown.
- Logging (`logging`): telemetry-level state
  and the panic hook (which shuts export
  down).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`telemetry.rs`](src/telemetry.rs) | lifecycle | [`telemetry/tests.rs`](src/telemetry/tests.rs) |
| [`logging.rs`](src/logging.rs) | level + hook | (via `telemetry` tests) |

## Public API

`init`, `session_context`, `otlp_active`,
`shutdown`, `FlushGuard`, and the `logging`
module (`init`, `debug_enabled`).

## How to verify

```sh
cargo nextest run -p jackin-usage-capsule-telemetry
cargo clippy -p jackin-usage-capsule-telemetry --all-targets -- -D warnings
```
