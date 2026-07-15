- Two-tier telemetry is the contract: `clog!` compact always-on, `cdebug!` firehose gated on `JACKIN_TELEMETRY_LEVEL=debug`, structured run/OTLP the third tier. New telemetry goes through the typed operation API (`operation_span`/`operation_log`/`operation_error`/`operation_metric`); `clog!`/`cdebug!`/`debug_log!` remain the console/file renderers and stay legal at existing sites; names come from the registry, never inline literals.
- Never leak secrets: any output that can hold credential/secret material goes through `redact`/`secret_scrub`.

## Boundaries

- Terminal-ownership *guards* live in `jackin-tui` (re-exported here). Usage/telemetry *store* + token monitors live in `jackin-usage`.
