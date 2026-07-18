- Application telemetry is registry-first and exports directly over OTLP. Emit only through `jackin-telemetry` governed events, operations, metrics, result ownership, and spawn helpers; legacy/generic telemetry macros and local telemetry renderers are prohibited. Operator output is a separate port, and `RunDiagnostics` holds bounded current-invocation UI state rather than telemetry history.
- Never leak secrets: any output that can hold credential/secret material goes through `redact`/`secret_scrub`.

## Boundaries

- Product terminal-ownership guards live in this crate's `terminal` module; TermRock owns neutral session mechanics. Usage snapshots and token monitors live in `jackin-usage`.
