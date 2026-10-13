# jackin-diagnostics

Composition root for jackin❯ observability and in-memory invocation progress.
Terminal-ownership and title policy for rich surfaces live in this crate's `terminal` module (product-owned process globals; TermRock owns neutral session mechanics).

## What this crate owns

- Direct OTLP/gRPC providers, stable process resources, current observations, retry, flush, and shutdown.
- Bounded current-invocation progress and timing for operator surfaces, never telemetry history.
- Operator output, secret scrubbing, and explicit workflow build-log capture.

The closed schema and governed emission APIs live in `jackin-telemetry`; do not bypass them.

## Architecture tier and allowed dependencies

**L2 infrastructure.** Architecture gates enforce dependencies. Product crates consume composition and operator-output services without OpenTelemetry SDK details.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root and re-exports | [`tests.rs`](src/tests.rs) |
| [`observability.rs`](src/observability.rs) · [`observability/`](src/observability) | OTLP configuration, providers, resources, health, retry, flush, shutdown | module tests and `tests/wire_*` |
| [`run.rs`](src/run.rs) | bounded in-memory invocation progress and timings | [`run/tests.rs`](src/run/tests.rs) |
| [`logging.rs`](src/logging.rs) · [`operator_notice.rs`](src/operator_notice.rs) | operator-output routing | module tests |
| [`secret_scrub.rs`](src/secret_scrub.rs) · [`redact.rs`](src/redact.rs) | defense-in-depth scrubbing | module tests |
| [`build_log.rs`](src/build_log.rs) | explicit workflow-owned build-log capture | [`build_log/tests.rs`](src/build_log/tests.rs) |

## Public API

Consumers initialize telemetry, inspect typed provider health, request marker/flush checks with per-signal exporter-success proof, manage invocation progress, and route operator notices. Instrumentation uses `jackin-telemetry`.

## How to verify

```sh
cargo nextest run -p jackin-diagnostics --all-features --locked
cargo clippy -p jackin-diagnostics --all-targets --all-features --locked -- -D warnings
```
