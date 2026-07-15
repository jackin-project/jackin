# Plan 003: Populate top-level OTLP `EventName` through the log bridge

> **Executor instructions**: Follow step by step; verify each step; STOP conditions are binding. Update the status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-diagnostics/`
> Mismatch with "Current state" = STOP. This plan additionally assumes plan 001 landed (registry + registered dotted names); verify `crates/jackin-diagnostics/src/registry.rs` exists before starting, else STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/codebase-health/001-telemetry-event-registry.md
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

The wire contract requires `EventName` as a non-empty, registered, dotted, low-cardinality **top-level** LogRecord field: "If the current tracing bridge cannot populate it, adapt or replace the bridge; a temporary `event.name` attribute must be converted and equality-tested." Today the name exists only as an `event.name` tracing field (an attribute after bridging), so backends cannot key on event type via the standard field, and nothing proves attribute and top-level field agree.

## Current state

- The OTLP log path bridges `tracing` events via `opentelemetry-appender-tracing` 0.32 (`crates/jackin-diagnostics/Cargo.toml:84`; wiring in `observability.rs` — host init composes `JackinDiagnosticsLayer` + bridge; capsule init at `observability.rs:921` uses `OpenTelemetryTracingBridge` directly).
- `event.name` is set as a tracing field in the emit macro (`observability.rs:1630` and sibling arms) and in `operation.rs` (`"event.name" = event_name`).
- opentelemetry 0.32's `LogRecord` API exposes `set_event_name(&mut self, name: …)` (check `opentelemetry::logs::LogRecord` trait in the vendored version via `cargo doc -p opentelemetry --no-deps` or docs.rs 0.32) — the appender may already map `event.name` attributes; VERIFY actual behavior first (step 1) rather than assuming.
- Exporter-backed tests read attributes only (e.g. `observability/otlp/tests.rs:227` expects `event.name == "compact.kind"` — post-001 this asserts registered dotted names).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Inspect bridge behavior | `cargo nextest run -p jackin-diagnostics --all-features -E 'test(/otlp/)'` | pass |
| Crate tests | `cargo nextest run -p jackin-diagnostics --all-features` | pass |
| Lint | `cargo clippy -p jackin-diagnostics --all-targets --all-features -- -D warnings` | exit 0 |

## Scope

**In scope**: `crates/jackin-diagnostics/src/observability.rs` (bridge wiring / custom layer), a new thin adapter layer if needed (self-named module + tests), `observability/otlp/tests.rs`.

**Out of scope**: changing event names themselves (001), Resource (002), capsule macro bodies (004), JSONL file schema (005). Do not fork or vendor `opentelemetry-appender-tracing`; prefer a wrapping `tracing_subscriber::Layer` or a `LogProcessor` that promotes the attribute.

## Git workflow

Branch `feat/otlp-top-level-eventname`; Conventional Commits; `git commit -s`; push after each commit.

## Steps

### Step 1: Establish what the bridge does today

Write a probe test in `observability/otlp/tests.rs` that emits one registered event and inspects the captured `SdkLogRecord`'s `event_name()` (the in-memory exporter exposes the record; check `opentelemetry_sdk::logs` test API). Record the observed behavior in the test name (`bridge_populates_top_level_event_name` or `bridge_leaves_event_name_empty_today`).

**Verify**: test compiles and documents actual behavior.

### Step 2: Promote the attribute to the top-level field

If the bridge doesn't populate it: add a `LogProcessor` wrapper (registered in the provider build at `observability.rs:826-833` region) that, on emit, reads the record's `event.name` attribute and calls `set_event_name` with it (registry-validated). Keep the `event.name` attribute as a mirror for now (contract allows the bridge-compat attribute until the Rust bridge writes EventName directly).

**Verify**: probe test now asserts `event_name()` equals the registered name.

### Step 3: Equality test + registry guard

Add a conformance assertion (pattern for plan 009): for every captured record with an `event.name` attribute, top-level `EventName` exists, is non-empty, equals the attribute, and `registry::lookup(name)` is `Some`. Capsule-path records are exercised after plan 004 — leave a named `#[ignore = "enabled by plan 004"]` test stub only if the capsule path cannot be exercised yet.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → all pass; `cargo xtask ci --fast` → exit 0.

## Test plan

Probe test (step 1), promotion test (step 2), equality/registry sweep (step 3) — all in `observability/otlp/tests.rs`, modeled on existing in-memory exporter captures.

## Done criteria

- [x] Captured records expose non-empty top-level `EventName` equal to the `event.name` attribute
- [x] EventName values validate against the plan-001 registry
- [x] `cargo nextest run -p jackin-diagnostics --all-features` exits 0; `cargo xtask ci --fast` exits 0
- [x] Status row updated

## STOP conditions

- opentelemetry 0.32 SDK/appender exposes no way to set or read `EventName` on the exported record (API absent) — report exact API surface found; a version bump decision belongs to the operator.
- Promoting the field requires forking the appender crate.
- Plan 001's registry is absent.

## Maintenance notes

- When the upstream appender learns to write EventName natively, delete the promotion processor and keep the equality test.
- Plan 009 folds the equality sweep into the CI conformance matrix.

## Execution notes

- Promote via `LogProcessor` (not appender fork); free-form names interned with `Box::leak` for attribute equality.
