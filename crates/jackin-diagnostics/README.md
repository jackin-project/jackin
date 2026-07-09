# jackin-diagnostics

Host observability substrate: structured JSONL run diagnostics, the debug-mode flag, the `debug_log!` macro, redaction/secret-scrubbing, build-log capture, and the run/summary/screen/terminal reporting helpers. The two-tier telemetry (`clog!` compact always-on, `cdebug!` firehose on `JACKIN_DEBUG=1`) is rooted here.

Terminal-ownership guards are re-exported from `jackin_tui::ownership`.

## What this crate owns

- Structured run diagnostics (`run`, `summary`, `observability`) and the debug-mode substrate (`debug_log`, `logging`).
- Secret scrubbing (`secret_scrub`, `redact`) so logs/telemetry never leak credentials.
- Build-log capture (`build_log`), operator notices (`operator_notice`), and screen/terminal reporting (`screen`, `terminal`).

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies: `jackin-core`, `jackin-tui` (terminal-ownership guard re-exports). Diagnostic code must not start calling presentation helpers beyond the guard re-exports.

## Structure

- `src/run.rs`, `src/summary.rs`, `src/observability.rs` — structured run diagnostics + OTLP tier
- `src/debug_log.rs`, `src/logging.rs` — `debug_log!`/`clog!`/`cdebug!` two-tier substrate
- `src/secret_scrub.rs`, `src/redact.rs` — redaction
- `src/build_log.rs`, `src/operator_notice.rs`, `src/screen.rs`, `src/terminal.rs` — reporting
- subdirs (`build_log/`, `debug_log/`, `secret_scrub/`, `observability/`, `redact/`, `operator_notice/`, `screen/`, `run/`, `summary/`) — module bodies + tests

## Public API

`debug_log!`/`clog!`/`cdebug!`, the run-diagnostics writer, redaction helpers, and the debug-mode flag — consumed by nearly every crate. Two-tier telemetry contract is documented in `ENGINEERING.md`.

## How to verify

```sh
cargo nextest run -p jackin-diagnostics
cargo clippy -p jackin-diagnostics --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
