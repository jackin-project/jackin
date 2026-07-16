# Plan 003: One-event-one-log subscriber layering — bridges, filters, and operator-output separation

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-diagnostics/src/logging.rs crates/jackin-diagnostics/src/operation.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (changes how every `tracing` event maps to OTel; conformance tests are the safety net)
- **Depends on**: plans/unified-otel-observability/002-otlp-composition-root.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the bridge/layering paragraphs of "Rust instrumentation architecture" ("`OpenTelemetryTracingBridge` converts governed `tracing` events…", "Operator output remains a separate typed product port…"); the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The roadmap contract requires: a `tracing` event's metadata name becomes the native OTel log `EventName`; `message` becomes an optional body; level becomes native severity; attributes stay typed; the active context supplies TraceId/SpanId; the `tracing-opentelemetry` layer converts **registered spans only**; per-layer target filters guarantee one event → one OTel log and **zero duplicate span events**; automatic error-event status and exception inference are disabled (the operation owner records status explicitly); and telemetry never renders to the operator surface. Today the subscriber stack in `otlp::init` composes the span layer, the logs bridge, and the JSONL layer with per-sink `EnvFilter`s, but events flow to BOTH the span layer (as span events) and the logs bridge, the tracing events use a `message` body with a separate `event.name` attribute instead of metadata names, and error status can be inferred. This plan fixes the plumbing layer so plans 004+ emit correctly-shaped signals.

## Current state

(verified at planning commit)

- `crates/jackin-diagnostics/src/observability.rs:848-905` — `otlp::init` composes: `tracing_opentelemetry::layer().with_tracer(tracer)` + `OpenTelemetryTracingBridge::new(&logger)` + `JackinDiagnosticsLayer` (JSONL), each `.with_filter(EnvFilter)` built by `export_filter_directive` (`:1002`) over `EXPORT_TARGETS` (`:974-1000`, global default `off` + per-target levels). `otlp::init_capsule` (`:907`) is the capsule variant plus a stderr fmt layer scoped to `opentelemetry*=warn` (`:939-944`).
- Events are emitted through big `tracing::event!` match ladders in `observability.rs:1687-2051` with field lists like `{ "parallax.run.id": …, "event.name": …, message = body }` on target `jackin_diagnostics` / `jackin_diagnostics::jsonl` (`JSONL_TARGET`, `:18`; `OPERATION_TARGET` in `operation.rs`).
- `crates/jackin-diagnostics/src/operation.rs:175-320` — `operation_log`/`operation_error` route through `registry::validate` then `tracing::event!`; `operation_error` sets span error status explicitly (`Span::current().set_status` at `:302`) — this explicit-owner pattern is the model the whole codebase moves to.
- The metadata name of a `tracing::event!` defaults to `event <file>:<line>` — nothing sets `otel.name`-style static names on log events today, so bridged `EventName`s are currently positional, not registered names.
- `crates/jackin-diagnostics/src/logging.rs` — the console tier: `stderr_line` (`:183`), deferred buffer, `emit_compact_line` (`:257`) also mirrors into the active run file. `TelemetrySink { OtlpSpans, OtlpLogs, Console, DiagnosticsFile }` (`:29`) with per-sink env levels (`sink_level`, `:65`).
- `tracing-opentelemetry` 0.33 layer options this plan relies on (confirm against the locked crate docs): `.with_error_records_to_exceptions(bool)`, `.with_error_events_to_status(bool)`, `.with_error_fields_to_exceptions(bool)`, and event→span-event conversion controlled by filtering events away from the layer (there is no single "disable span events" switch — the filter is the mechanism).

## Target layering (the contract)

```
tracing Registry
├─ tracing-opentelemetry span layer
│    filter: SPANS ONLY (all events filtered out) + registered span targets
│    options: error_events_to_status=false, error_records_to_exceptions=false,
│             error_fields_to_exceptions=false
├─ OpenTelemetryTracingBridge (logs)
│    filter: EVENTS ONLY, governed targets (the facade target(s) from plan 004,
│            plus interim legacy targets until plans 011/013 drain them)
└─ [interim] JackinDiagnosticsLayer (JSONL fallback) — untouched here, deleted in plan 013
```

- No fmt/formatter layer over application targets in any product binary (the capsule's `opentelemetry*=warn` stderr diag layer is allowed to stay until plan 013 — it watches exporter internals, not app targets, and exporter `internal-logs` is disabled by plan 002 anyway; remove it if it no longer receives anything).
- Every governed event carries a static metadata name = the registered event name, so the bridge maps it to native `EventName`. With `tracing::event!` this means using the `name:` position: `tracing::event!(name: "ui.screen.entered", target: EVENTS_TARGET, Level::INFO, { … })`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-diagnostics --all-features --locked` | all pass |
| Conformance | `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` | all pass |
| Export-volume regen (if counts change) | `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` then `cargo xtask lint ratchet --print export-volume` | ratchet rows printed; paste into `ratchet.toml` |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Lint | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-diagnostics/src/observability.rs` (layer composition, filters, event ladders' names), new `observability/layering.rs` if splitting helps stay under the file-size ratchet
- `crates/jackin-diagnostics/src/operation.rs` (static names on emitted events)
- `crates/jackin-usage/src/telemetry.rs` (`bridge_log_structured` events get static names: `capsule.log`/`capsule.debug`/`capsule.warn`/`capsule.error`/`capsule.trace` as metadata names instead of `event.name` attrs)
- `ratchet.toml` export-volume rows (counts may legitimately change when duplicate span events disappear)

**Out of scope:**
- New facade API (plan 004). New event names beyond making existing ones static.
- Deleting the JSONL layer or legacy sinks (plan 013).
- Any call-site migration in product crates.

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `fix(diagnostics): route events to logs bridge only with native event names`. Sign `-s`, push after every commit.

## Steps

### Step 1: Split span-layer vs logs-bridge filtering

Rework the filters in `otlp::init`/`otlp::init_capsule` so:
- the `tracing-opentelemetry` layer receives spans from governed targets and **no events** — build its `EnvFilter` from span targets and combine with a `filter_fn` on metadata kind (`metadata.is_span()`);
- the logs bridge receives events from governed targets and **no spans** (bridge only observes events by construction; the target filter still applies);
- disable inference on the span layer: `.with_error_records_to_exceptions(false).with_error_events_to_status(false).with_error_fields_to_exceptions(false)` (exact method names per tracing-opentelemetry 0.33 — check the crate docs; if a listed toggle does not exist in 0.33, note which and rely on the event filter for that behavior).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features --locked` → the existing OTLP tests still pass except any asserting span events — update those to assert **zero** span events for bridged log events (see Test plan).

### Step 2: Native EventName on every governed event

Convert `tracing::event!` sites in `observability.rs:1687-2051`, `operation.rs` (`:223,234,245,288` region), and `jackin-usage/src/telemetry.rs` (`:160-204` region) to the `name:` form using the registered name that today travels in the `event.name` field. Keep the `event.name` attribute temporarily (the JSONL adapter and Parallax-side compat read it; plan 013 removes it with the JSONL layer). `message` stays the redacted body.

**Verify**: new test — export one operation log via the in-memory `TestExport` and assert the OTel `LogRecord.event_name()` equals the registered name (0.32 SDK exposes event name on the record; if the accessor differs, assert via the exported record's fields).

### Step 3: One-event-one-signal conformance test

Add `conformance_single_delivery` in `crates/jackin-diagnostics/src/observability/otlp/tests.rs`: emit one governed event inside an active operation span; assert exactly one OTel log record with correct EventName/severity/TraceId/SpanId, zero span events on the exported span, and (until plan 013) one JSONL line. Assert ERROR-level event does NOT flip span status (status only via the explicit `operation_error`/guard path).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features --locked -E 'test(conformance_single_delivery)'` → passes.

### Step 4: Severity + TRACE gate check

Assert level→severity mapping (INFO/WARN/ERROR/DEBUG/TRACE) through the bridge, and that TRACE-level governed events only pass the filter when the trace gate is on (`sink_level` for the Otlp sinks resolves TRACE only under `JACKIN_TELEMETRY_LEVEL=trace` — existing behavior at `logging.rs:46-82`; do not weaken it).

**Verify**: targeted tests pass; `cargo nextest run --workspace --all-features --locked` → all pass.

### Step 5: Export-volume ratchet

If default-mode signal counts changed (removed duplicate span events can lower them), regenerate per the command table and update `ratchet.toml` rows (`default_mode_logs`, `default_mode_spans`, `default_mode_metrics` near lines 643-662).

**Verify**: `cargo xtask lint --strict` → exit 0.

## Reopened audit additions (2026-07-16)

- Remove the compatibility path that re-enters `RunDiagnostics` and exports a second/spurious log. A compatibility API may remain only if it preserves the registered EventName and every typed caller field without loss.
- Generate per-event emission from the schema's required/allowed fields so strings, booleans, signed/unsigned integers, floats, and arrays all round-trip; hard-coded severity-specific field subsets are forbidden.
- Reject unknown governed EventName and span names before export and count `unknown_name`; callers cannot forge a registered event at a noncanonical severity.
- One focused event-inside-span test asserts exact native EventName, native severity, body, typed fields, immutable Resource, and matching TraceId/SpanId. INFO/WARN/ERROR/DEBUG/TRACE filter matrices also assert exactly one log, zero duplicate span events, no automatic status/exception inference, and the explicit DEBUG/TRACE gates.

## Test plan

- `conformance_single_delivery` (step 3) — the load-bearing test of this plan.
- EventName mapping test (step 2).
- Severity mapping + TRACE gating tests (step 4).
- Regression: existing ~40 tests in `observability/otlp/tests.rs` updated where they asserted span-event duplication or inferred status; each such change must be justified by the contract (comment referencing the roadmap section).
- Pattern to model on: existing tests use `TestExport::force_flush` + in-memory exporter assertions (`observability_test_support.rs`).

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] `conformance_single_delivery` exists and passes
- [ ] `grep -n "with_error_events_to_status\|error_records_to_exceptions" crates/jackin-diagnostics/src/` shows inference disabled (or documented 0.33 equivalent)
- [ ] `cargo xtask lint --strict` exits 0 (export-volume ratchet consistent)
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- tracing-opentelemetry 0.33 cannot filter events away from the span layer without also losing spans (i.e. the `filter_fn`+`EnvFilter` combination misbehaves) — report the observed behavior with a minimal repro.
- The 0.32 logs bridge does not map metadata name → EventName (verify early with a spike test before converting all ladders).
- Export-volume deltas exceed ±10 signals (suggests you broke more than duplication).

## Maintenance notes

- Plan 004's facade emits through the target(s) whose filters you define here — keep target constants (`EXPORT_TARGETS`) in one place and documented.
- Plan 013 deletes the JSONL layer and the interim `event.name` attribute; leave `// TODO(otel-cutover)` markers at both.
- Reviewer focus: every `tracing::event!` in the diagnostics crate now has a static `name:`; grep `tracing::event!` and confirm no anonymous events remain on governed targets.
