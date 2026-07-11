# jackin-diagnostics

Host observability substrate: structured JSONL run diagnostics, the debug-mode flag, the `debug_log!` macro, redaction/secret-scrubbing, build-log capture, and the run/summary/screen/terminal reporting helpers. The two-tier telemetry (`clog!` compact always-on, `cdebug!` firehose on `JACKIN_DEBUG=1`) is rooted here.

Terminal-ownership guards are re-exported from `jackin_tui::ownership`.

## What this crate owns

- Structured run diagnostics (`run`, `summary`, `observability`) and the debug-mode substrate (`debug_log_adapter` installing the `jackin-core` `debug_log!` sink, plus `logging`).
- Secret scrubbing (`secret_scrub`, `redact`) so logs/telemetry never leak credentials.
- Build-log capture (`build_log`), operator notices (`operator_notice`), and screen/terminal reporting (`screen`, `terminal`).

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies: `jackin-core`, `jackin-tui` (terminal-ownership guard re-exports). Diagnostic code must not start calling presentation helpers beyond the guard re-exports.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`run.rs`](src/run.rs) · [`run/`](src/run) | structured run diagnostics | [`tests.rs`](src/run/tests.rs) |
| [`summary.rs`](src/summary.rs) · [`summary/`](src/summary) | run summary | [`tests.rs`](src/summary/tests.rs) |
| [`operation.rs`](src/operation.rs) · [`operation/`](src/operation) | typed operation facade | [`tests.rs`](src/operation/tests.rs) |
| [`conformance.rs`](src/conformance.rs) · [`conformance/`](src/conformance) | telemetry acceptance scenario | [`tests.rs`](src/conformance/tests.rs) |
| [`metrics.rs`](src/metrics.rs) · [`metrics/`](src/metrics) | hot-path metric instruments | [`tests.rs`](src/metrics/tests.rs) |
| [`observability.rs`](src/observability.rs) · [`observability/`](src/observability) | OTLP tier | [`tests.rs`](src/observability/tests.rs) |
| [`debug_log_adapter.rs`](src/debug_log_adapter.rs) | host sink install for `jackin-core::debug_log!` | — |
| [`logging.rs`](src/logging.rs) | logging init | — |
| [`secret_scrub.rs`](src/secret_scrub.rs) · [`secret_scrub/`](src/secret_scrub) | secret scrubbing | [`tests.rs`](src/secret_scrub/tests.rs) |
| [`redact.rs`](src/redact.rs) · [`redact/`](src/redact) | redaction | [`tests.rs`](src/redact/tests.rs) |
| [`build_log.rs`](src/build_log.rs) · [`build_log/`](src/build_log) | build-log capture | [`tests.rs`](src/build_log/tests.rs) |
| [`operator_notice.rs`](src/operator_notice.rs) · [`operator_notice/`](src/operator_notice) | operator notices | [`tests.rs`](src/operator_notice/tests.rs) |
| [`screen.rs`](src/screen.rs) · [`screen/`](src/screen) | screen reporting | [`tests.rs`](src/screen/tests.rs) |
| [`terminal.rs`](src/terminal.rs) | terminal reporting | — |
| [`tests.rs`](src/tests.rs) | integration tests | — |

## Public API

Typed operation facade: `operation_span` / `operation_log` / `operation_error` / `operation_metric` (and `enter_operation` RAII guard). Names from the semconv registry.

`debug_log!`/`clog!`/`cdebug!`, the run-diagnostics writer, redaction helpers, and the debug-mode flag — consumed by nearly every crate. Two-tier telemetry contract is documented in `ENGINEERING.md`.

## How to verify

```sh
cargo nextest run -p jackin-diagnostics
cargo clippy -p jackin-diagnostics --all-targets -- -D warnings
cargo bench --bench summarize_jsonl -p jackin-diagnostics -- --quick
```

