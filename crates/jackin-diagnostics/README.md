# jackin-diagnostics

Host observability substrate: structured JSONL run diagnostics, the debug-mode flag, the `debug_log!` macro, redaction/secret-scrubbing, build-log capture, and the run/summary/screen/terminal reporting helpers. Its two tiers are `clog!` compact always-on and `cdebug!` firehose at telemetry debug.

Terminal-ownership and title policy for rich surfaces live in this crate's `terminal` module (product-owned process globals; TermRock owns neutral session mechanics).

## What this crate owns

- Structured run diagnostics (`run`, `summary`, `observability`) and the debug-mode substrate (`debug_log_adapter` installing the `jackin-core` `debug_log!` sink, plus `logging`).
- Secret scrubbing (`secret_scrub`, `redact`) so logs/telemetry never leak credentials.
- Build-log capture (`build_log`), operator notices (`operator_notice`), and screen/terminal reporting (`screen`, `terminal`).

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies: `jackin-core`. Terminal ownership flags stay product-local here; neutral TUI presentation is TermRock and must not be pulled into diagnostics.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`run.rs`](src/run.rs) · [`run/`](src/run) | structured run diagnostics | [`tests.rs`](src/run/tests.rs) |
| [`summary.rs`](src/summary.rs) · [`summary/`](src/summary) | run summary | [`tests.rs`](src/summary/tests.rs) |
| [`operation.rs`](src/operation.rs) · [`operation/`](src/operation) | typed operation facade | [`tests.rs`](src/operation/tests.rs) |
| [`metrics.rs`](src/metrics.rs) · [`metrics/`](src/metrics) | hot-path metric instruments | [`tests.rs`](src/metrics/tests.rs) |
| [`observability.rs`](src/observability.rs) · [`observability/`](src/observability) | OTLP tier | [`tests.rs`](src/observability/tests.rs) |
| [`registry.rs`](src/registry.rs) · [`registry/`](src/registry) | fail-closed event registry + attribute schema | [`tests.rs`](src/registry/tests.rs) |
| [`debug_log_adapter.rs`](src/debug_log_adapter.rs) | host sink install for `jackin-core::debug_log!` | — |
| [`logging.rs`](src/logging.rs) | logging init | — |
| [`secret_scrub.rs`](src/secret_scrub.rs) · [`secret_scrub/`](src/secret_scrub) | secret scrubbing | [`tests.rs`](src/secret_scrub/tests.rs) |
| [`redact.rs`](src/redact.rs) · [`redact/`](src/redact) | redaction | [`tests.rs`](src/redact/tests.rs) |
| [`build_log.rs`](src/build_log.rs) · [`build_log/`](src/build_log) | build-log capture | [`tests.rs`](src/build_log/tests.rs) |
| [`operator_notice.rs`](src/operator_notice.rs) · [`operator_notice/`](src/operator_notice) | operator notices | [`tests.rs`](src/operator_notice/tests.rs) |
| [`screen.rs`](src/screen.rs) · [`screen/`](src/screen) | screen reporting | [`tests.rs`](src/screen/tests.rs) |
| [`terminal.rs`](src/terminal.rs) | terminal reporting | — |
| [`tests.rs`](src/tests.rs) | crate integration and telemetry-conformance scenarios | — |

## Public API

Typed operation facade: `operation_span` / `operation_log` / `operation_error` / `operation_metric` (and `enter_operation` RAII guard). Names from the semconv registry.

`debug_log!`/`clog!`/`cdebug!`, the run-diagnostics writer, redaction helpers, and the debug-mode flag — consumed by nearly every crate. The two-tier telemetry contract is documented in [ENGINEERING.md](../../ENGINEERING.md).

## How to verify

```sh
cargo nextest run -p jackin-diagnostics
cargo clippy -p jackin-diagnostics --all-targets -- -D warnings
cargo bench --bench summarize_jsonl -p jackin-diagnostics -- --quick
```
