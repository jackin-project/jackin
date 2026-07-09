- Two-tier telemetry is the contract: `clog!` compact always-on, `cdebug!` firehose gated on `JACKIN_DEBUG=1`, structured run/OTLP the third tier. New logging uses these macros, not `log::`/`tracing::` directly.
- Never leak secrets: any output that can hold credential/secret material goes through `redact`/`secret_scrub`.

## Boundaries

- Terminal-ownership *guards* live in `jackin-tui` (re-exported here). Usage/telemetry *store* + token monitors live in `jackin-usage`.
