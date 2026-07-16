# jackin-diagnostics

Host composition root for jackin❯ observability and bounded in-memory invocation progress.

## What this crate owns

- Direct OTLP/gRPC provider construction, stable process resources, current provider observations, retry classification, flush, and shutdown.
- Bounded current-invocation progress and timing state used by operator surfaces; this state is never a telemetry history store.
- Operator-output routing, secret scrubbing, and explicit build-log capture requested by product workflows.

The closed schema and all governed emission APIs live in `jackin-telemetry`. This crate must not invent signal names or bypass that facade.

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies are enforced by the architecture-tier gate. Product crates may consume composition and operator-output services without depending on OpenTelemetry SDK details.

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

Consumers initialize process telemetry, inspect current provider state, request marker/flush validation, manage bounded invocation progress, and route operator notices. Positive per-signal backend delivery proof and the final typed health contract remain pending. Instrumentation uses `jackin-telemetry` directly.

## How to verify

```sh
cargo nextest run -p jackin-diagnostics --all-features --locked
cargo clippy -p jackin-diagnostics --all-targets --all-features --locked -- -D warnings
```
