# AGENTS.md — jackin-diagnostics

Host observability substrate: structured JSONL run diagnostics, debug-mode, `debug_log!`/`clog!`/`cdebug!`, redaction.

## Hard rules (this crate)

- **Tier & dependencies:** L2 infrastructure. Allowed workspace deps: `jackin-core`, `jackin-tui` (for terminal-ownership guard re-exports only). Diagnostic code must not call presentation helpers beyond those re-exports.
- **Keep `README.md` current:** update it when structure, public API, the telemetry macros, or the run-diagnostics format change (see `crates/AGENTS.md`).
- **Two-tier telemetry is the contract.** `clog!` = compact always-on; `cdebug!` = firehose gated on `JACKIN_DEBUG=1`; structured run/OTLP is the third tier. New logging uses these macros — not `log::`/`tracing::` directly (candidate dylint lint, see code-health roadmap).
- **Never leak secrets.** All credential/secret-bearing output goes through `redact`/`secret_scrub`; a new log field that can hold secret material must be scrubbed.

## What lives here vs elsewhere

- This crate owns: run diagnostics, debug substrate, telemetry macros, redaction, build-log capture, operator notices, screen/terminal reporting.
- Terminal-ownership *guards* live in `jackin-tui` (re-exported here). Usage/telemetry *store* + token monitors live in `jackin-usage`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
