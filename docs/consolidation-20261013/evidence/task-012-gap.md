# TASK-012 gap audit

Status: NOT APPROVED. Read-only. No product edits. `cargo` and `mbx` were not run.

## Scope

| Anchor | SHA | Check |
| --- | --- | --- |
| Integration `wt-integration-20261013` | `738e2cfa95731b8ee59e705f52e38c802c12a3a8` | `merge-base --is-ancestor 738e2cfa HEAD` exit 0. This is the required floor, not a later commit. |
| Policy P `wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` | HEAD matches. P is not an ancestor of integration. `git diff fba69d60 --` the four owned trees is empty. |
| Per-request assertion | `502e0627d3940e2cd68ae0078910f21452d9ccc1` | Shown with `git -C jackin show`. Not checked out. One file, `crates/jackin-diagnostics/tests/wire_failure_support/mod.rs`, +14/−3. Byte-identical to the integration file below. |
| Docker split | `b72046bbc9314e93cddf4d8252570dc375935f56` | Shown with `git show`. Not checked out. Parent `fb52c525d3561406f9d6d5f3a2f0b8918611d0f8`. |

Owned trees searched: `crates/services/jackin-diagnostics/`, `crates/services/jackin-telemetry/`, `crates/testing/jackin-otlp-testbed/`, `crates/adapters/jackin-docker/` tests.

## Verdicts

### 1. Diagnostic redaction does not leak secrets, PTY bytes, or sensitive bodies — ALREADY_PRESENT

`redact_text` is the production helper at `crates/services/jackin-diagnostics/src/redact.rs:526`.

Bodies and exception text go through it:

- `crates/services/jackin-telemetry/src/event.rs:313` clamps the log body with the installed redactor.
- `crates/services/jackin-telemetry/src/event.rs:318` redacts `exception.message` and `exception.stacktrace`.
- `crates/services/jackin-diagnostics/src/observability.rs:219`, `:299`, and `crates/services/jackin-diagnostics/src/observability/otlp.rs:1126` install `redact_text`.
- `crates/services/jackin-diagnostics/src/observability.rs:539` redacts progress messages before emit.

Secret and sensitive-body assertions are live, not name-only:

- `crates/services/jackin-diagnostics/src/tests/support.rs:187` submits the argv, URL, and inspect canaries as one operation-log body.
- `crates/services/jackin-diagnostics/src/tests/case_01.rs:213` fails if those canaries, or the terminal canary, appear in the export dump, and `:228` calls `redact_text` directly.
- `crates/services/jackin-diagnostics/tests/wire_support/mod.rs:136` lists path, token, authorization, password, role, container, tab, `fixture-pty-bytes`, and mouse-coordinate canaries.
- `crates/testing/jackin-otlp-testbed/src/lib.rs:785` scans log-record bodies, so a sensitive body on the wire is a violation.

PTY bytes are not an exported payload:

- `crates/services/jackin-diagnostics/src/metrics.rs:31` records a byte count only.
- `crates/services/jackin-telemetry/src/privacy.rs:32` rejects attribute strings that contain ESC, newlines, secret assignments, URLs, or absolute paths.
- `crates/services/jackin-telemetry/src/privacy/tests.rs:18` includes `\u{1b}[31mterminal payload`.

`CONFORMANCE_TERMINAL_CANARY` at `crates/services/jackin-diagnostics/src/tests/support.rs:17` is asserted at `case_01.rs:221` and is not passed into `conformance_operation_log`. That is the same text as policy P. It is an absence canary. It is not a missing `redact_text` hole: attributes already fail closed on terminal payloads, and the metric path does not copy PTY bytes.

Rejected redaction intermediates stay out. `plans/jackin-build-split/branches.md:70`, `:71`, and `:81` reject `08135f1ae010c63cec1bd7ff3a8125036a5c1576`, `63d5ef9046d4948a3cddb239e891db49be654d34`, and `69b82de1a48cc18add3933e1995028c8aa2722e8`. **REJECT** importing those implementations.

The synthetic receiver is not a production fallback. `init_wire_test_export` is `#[cfg(feature = "test-support")]` at `observability.rs:238`. Default features are empty at `crates/services/jackin-diagnostics/Cargo.toml:21`. `init_tracing_for` at `observability.rs:221` exports only when standard OTEL config resolves.

### 2. Each retried log request contains exactly one matching record — ALREADY_PRESENT

`assert_wire_requests` at `crates/services/jackin-diagnostics/tests/wire_failure_support/mod.rs:60` requires every captured log request to contain exactly one record whose event name is `telemetry.validate` (`:75`–`:83`).

The retry caller is `crates/services/jackin-diagnostics/tests/wire_failure_retryable.rs:6`, which expects three requests. Partial success and unauthenticated responses expect one request at `wire_failure_partial_success.rs:6` and `wire_failure_non_retryable.rs:6`.

No equivalent helper exists in `jackin-telemetry`, `jackin-otlp-testbed`, or `jackin-docker`. The diagnostics helper is the selected one, and it matches `502e0627` byte for byte.

### 3. A corrupt 0/1/2 distribution fails even when the total matches — ALREADY_PRESENT

The same predicate is the negative check. Flattened `log_records.len() == expected_requests` at `wire_failure_support/mod.rs:73` would accept 0+1+2 = 3. `records.len() == 1` at `:82` rejects the 0-record and 2-record requests, so `records_match` is false and the assert at `:131` fails.

Search of the four trees found no second test that builds a synthetic 0/1/2 `Testbed`. `502e0627` did not add one either; its whole change is this predicate and the failure message at `:69`–`:83` and `:131`–`:142`. A new fixture would duplicate the source assertion. **Not PORT.**

### 4. Docker TLS feature guard and incompatible connection choices remain, without obsolete imports — ALREADY_PRESENT

The compile-time TLS guard is `crates/adapters/jackin-docker/src/docker_client/tests/support.rs:5`: `Docker::connect_with_ssl_defaults`. Bollard is built with `aws-lc-rs` at `crates/adapters/jackin-docker/Cargo.toml:26`. Dropping that feature fails this const.

Rejection of incompatible choices is in product code at `crates/adapters/jackin-docker/src/docker_client.rs:199`–`:207` (SSH, HTTPS/TLS transport, TLS material, skip-verify) and in tests:

- `tests/case_01.rs:110` SSH
- `:118` HTTPS
- `:126` TLS material
- `:141` skip-verify
- `:156` unknown URI
- `:173` npipe on non-Windows
- `:211` SSH over TLS material
- `:232` and `:242` unsupported-message text

Against `b72046bb`, `tests.rs`, `tests/case_01.rs`, and `tests/support.rs` differ only by `pub(super)` on helpers and blank lines. The split’s `use super::*` remains. The pre-split monolith and its dropped comment block were not restored. **REJECT** restoring obsolete imports or the old single-file module.

## Verification

BLOCKED / NOT_RUN. This audit was forbidden to run `cargo` or `mbx`, so the TASK-012 `nextest` and `clippy` commands were not executed. Test presence is not a pass.

## Smallest edit list

Empty.
