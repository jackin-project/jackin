# Plan 001: Add an in-memory OTLP export test seam and characterize the current export contract

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

jackin❯ exports logs, spans, and metrics over OTLP to an OpenTelemetry backend (Parallax is the reference), but **not one test in the workspace asserts what an exported record looks like** — no test captures severity, body, attributes, span names, or resource attributes on the wire. The whole export path (`crates/jackin-diagnostics/src/observability.rs`) is built with real tonic gRPC exporters and has no injection seam, so it is structurally untestable today. Plans 002–012 all restructure this export path; without this seam, every one of them can silently regress the export contract (collapse ERROR→INFO, drop the body, lose `parallax.run.id`) and CI stays green. This plan adds the seam and pins the *current* behavior with characterization tests, so later plans change behavior deliberately, updating tests as part of their diff.

## Current state

Files:

- `crates/jackin-diagnostics/src/observability.rs` — subscriber setup; `mod otlp` (behind `otlp` cargo feature) builds exporters/providers; `emit_jsonl_event_with_level` at the bottom is the log-emit choke point.
- `crates/jackin-diagnostics/src/run.rs` — `RunDiagnostics` (run identity, JSONL file sink, stage/timing APIs).
- `crates/jackin-diagnostics/src/observability/otlp/tests.rs` — existing tests: endpoint resolution, protocol guard, `build_resource` attrs, `OtelInternalVisitor`. Nothing captures an exported record.
- `crates/jackin-diagnostics/Cargo.toml` — dev-dependencies today: `tempfile` only.

Host provider construction (`observability.rs:753-772`, inside `mod otlp`):

```rust
let span_exporter = opentelemetry_otlp::SpanExporter::builder()
    .with_tonic()
    .with_endpoint(endpoints.traces.clone())
    .build()
    .map_err(|e| anyhow::anyhow!("OTLP span exporter init failed: {e}"))?;
// ... LogExporter likewise ...
let tracer_provider = SdkTracerProvider::builder()
    .with_span_processor(BatchSpanProcessor::builder(span_exporter, Tokio).build())
    .with_resource(resource.clone())
    .build();
let logger_provider = SdkLoggerProvider::builder()
    .with_log_processor(BatchLogProcessor::builder(log_exporter, Tokio).build())
    .with_resource(resource.clone())
    .build();
```

Layer composition (`observability.rs:788-806`): `tracing_opentelemetry::layer().with_tracer(tracer)` + `OpenTelemetryTracingBridge::new(&logger_provider)`, each `.with_filter(EnvFilter::new(directive))` where the directive is built at `observability.rs:797-800`.

Log emit choke point (`observability.rs:1155-1203`): `emit_jsonl_event_with_level` emits `tracing::error!/debug!/info!` on `target: JSONL_TARGET` with fields `jackin_jsonl`, `run_id`, `kind`, `diagnostics_message`, `stage`, `detail` and format message `"{message}"`. Severity mapping: `JsonlEventLevel::Error` → `error!`, `kind == "debug"` → `debug!`, else `info!`.

The `opentelemetry_sdk` crate (already a dependency, version 0.32, `crates/jackin-diagnostics/Cargo.toml:73`) ships in-memory exporters under its `testing` feature: `opentelemetry_sdk::testing::trace::InMemorySpanExporter` and `opentelemetry_sdk::testing::logs::InMemoryLogExporter` (types may be exposed as `InMemorySpanExporterBuilder`/`InMemoryLogExporterBuilder`; check the 0.32 docs of the pinned version). They work with the simple (synchronous) processors, which avoids the dedicated tokio runtime entirely in tests.

Repo conventions that apply:

- Tests live in `<module>/tests.rs` beside the module; never inline `#[cfg(test)] mod tests { … }` blocks in source files; never `mod.rs` (see `crates/AGENTS.md`).
- Test runner is `cargo nextest run` — **never `cargo test`** (TESTING.md). nextest runs each test in its own process, which is what makes global-subscriber tests viable.
- `tracing_subscriber::registry().try_init()` can only succeed once per process — one test per process may install a global subscriber. Prefer `tracing::subscriber::with_default` (scoped subscriber) so multiple tests in one binary don't fight; nextest process-per-test is the backstop.
- Workspace lints deny `unwrap_used`/`expect_used` in non-test code; tests may use them freely (they are `#[cfg(test)]`).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cargo fmt --check` | exit 0 |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Typecheck | `cargo check --all-targets --all-features` | exit 0 |
| Tests (crate) | `cargo nextest run -p jackin-diagnostics --all-features` | all pass |
| Tests (workspace) | `cargo nextest run --all-features` | all pass |

## Scope

**In scope** (the only files you should modify):

- `crates/jackin-diagnostics/Cargo.toml` (add dev-dependency feature)
- `crates/jackin-diagnostics/src/observability.rs` (add a test-only layer-construction helper)
- `crates/jackin-diagnostics/src/observability/otlp/tests.rs` (new tests)
- `crates/jackin-diagnostics/src/run/tests.rs` (only if a helper is needed there)

**Out of scope** (do NOT touch, even though they look related):

- The EnvFilter directive strings and any production filter behavior (plan 002 changes them; here you only pin them).
- `emit_jsonl_event_with_level` semantics, severity mapping, attribute set (plans 003/004 change them deliberately).
- `crates/jackin-usage/` and the capsule bridge (plan 003).
- Any production (non-`#[cfg(test)]`) behavior change at all. This plan is additive test infrastructure; the only production-file edits are a test-only constructor/helper.

## Git workflow

- This repo forbids committing to `main`. Propose branch `test/otlp-export-seam` to the operator and wait for confirmation before creating it (root `AGENTS.md` hard rule).
- Conventional Commits, DCO sign-off, push after every commit: `git commit -s -m "test(diagnostics): add in-memory OTLP export seam and contract tests"` then `git push`.

## Steps

### Step 1: Enable the SDK testing feature as a dev-dependency

In `crates/jackin-diagnostics/Cargo.toml`, add to `[dev-dependencies]`:

```toml
opentelemetry_sdk = { version = "0.32", features = ["testing", "trace", "logs"] }
```

Keep the existing optional `[dependencies]` entry untouched. If cargo rejects the feature name `testing`, run `cargo doc -p opentelemetry_sdk --no-deps` or check `~/.cargo/registry` sources for the pinned 0.32 version to find the exact feature/testing-module name — it exists in the 0.3x line; if it truly does not exist in the pinned version, STOP and report.

**Verify**: `cargo check -p jackin-diagnostics --all-features --all-targets` → exit 0.

### Step 2: Add a test-only layer builder in `observability.rs`

Add, inside `mod otlp` (so it is `#[cfg(feature = "otlp")]`), a `#[cfg(test)]`-gated helper that builds the *same* layer stack as `init()` but on top of in-memory exporters and returns the pieces a test needs:

```rust
#[cfg(test)]
pub(super) struct TestExport {
    pub(super) spans: opentelemetry_sdk::testing::trace::InMemorySpanExporter,
    pub(super) logs: opentelemetry_sdk::testing::logs::InMemoryLogExporter,
    pub(super) tracer_provider: SdkTracerProvider,
    pub(super) logger_provider: SdkLoggerProvider,
}

#[cfg(test)]
pub(super) fn test_layers(debug: bool, run_id: &str) -> (TestExport, impl tracing::Subscriber) { ... }
```

Requirements:

- Use simple processors (`SdkTracerProvider::builder().with_simple_exporter(spans.clone())` / logger equivalent) — no tokio runtime, no batch.
- Stamp the same resource as production via the existing `build_resource(run_id)`.
- Compose `tracing_subscriber::registry()` with `JackinDiagnosticsLayer` + span layer + log bridge, **using the same directive string the production `init()` builds** — extract the directive construction into a small function (e.g. `fn export_filter_directive(level: &str) -> String`) called by both `init`, `init_capsule`, and the test helper, so the test pins the real filter. This extraction is a pure refactor (same output string); it also removes the host/capsule copy-paste of the directive, which plan 002 then edits in one place.
- Return the subscriber unset; tests run bodies under `tracing::subscriber::with_default(subscriber, || { ... })` and then `force_flush()` the providers before asserting.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → existing tests still pass.

### Step 3: Write the characterization tests

In `crates/jackin-diagnostics/src/observability/otlp/tests.rs`, add tests that pin CURRENT behavior (do not "fix" anything you observe — these tests are the baseline later plans edit deliberately):

1. `exported_log_carries_body_and_attributes` — under the test subscriber, call `crate::observability::emit_jsonl_event("run1", "compact_kind", "hello world", Some("plan"), Some("d"))`... note `emit_jsonl_event` is `pub(crate)`, reachable from this module. Flush, then assert on `logs.get_emitted_logs()`: exactly one record; severity == Info; body text == `"hello world"`; attributes contain `kind="compact_kind"`, `stage="plan"`, `detail="d"`, `run_id="run1"`, `diagnostics_message="hello world"`, `jackin_jsonl=true`. (Yes — the duplicate body and the marker attr are current behavior; pin them. Plan 004 will change this test.)
2. `exported_error_log_is_error_severity` — `emit_jsonl_error(...)` → severity == Error.
3. `debug_kind_is_debug_severity_and_filtered_at_info` — with `test_layers(false /* info */)`, emit a `kind="debug"` event and assert **zero** exported logs (info-level filter drops DEBUG); with `test_layers(true)`, assert one DEBUG record whose `stage` attribute is absent and `detail` carries the category (current shape from `run.rs:480`).
4. `sentinel_none_values_are_exported` — emit with `stage=None` and assert the exported record carries the literal attribute `stage="<none>"` (current behavior; plan 004 removes it — this test documents it).
5. `launch_stage_span_name_is_constant` — create `tracing::info_span!("launch_stage", stage = "derived image")` under the test subscriber, enter+drop it, flush, assert `spans.get_finished_spans()` has one span named `launch_stage` with attribute `stage="derived image"`. (Plan 007 changes the exported name via `otel.name`; it will update this test.)
6. `dependency_targets_pass_the_filter_today` — emit `tracing::info!(target: "turso_core", "vm step")` under `test_layers(false)` and assert it IS exported (one log record). This is the bug plan 002 fixes; pinning it proves the fix flips this exact assertion.
7. `resource_carries_run_id_service_and_component` — assert the resource on an exported record/span carries `service.name="jackin"`, `jackin.component="host"`, `parallax.run.id=<run_id>` (extend the existing `build_resource` tests to the wire level).
8. `format_parse_traceparent_roundtrip` — `parse_traceparent` (`observability.rs:952-970`) is currently untested: valid header roundtrip with `screen.rs`'s `format_traceparent` output shape (`00-<32hex>-<16hex>-01`), plus rejects: wrong version (`01-...`), missing segment, extra segment, non-hex trace id. `parse_traceparent` is private to `mod otlp` — the tests file is `observability/otlp/tests.rs`, a child of that module, so it is reachable as `super::parse_traceparent`.

Use the existing tests in that file as the structural pattern (plain `#[test]` fns, no fixtures).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → all pass, including 8+ new tests.

### Step 4: Workspace green

**Verify**: `cargo fmt --check` → exit 0; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo nextest run --all-features` → all pass.

## Test plan

This plan IS the test plan — Steps 3's eight named tests. Happy path: log body/attrs/severity/resource on the wire. Regression pins: sentinel attrs, duplicate `diagnostics_message`, dependency-target passthrough, constant `launch_stage` name (each explicitly referenced by the later plan that changes it).

## Done criteria

- [ ] `cargo nextest run -p jackin-diagnostics --all-features` exits 0 with ≥8 new tests
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0
- [ ] `cargo fmt --check` exits 0
- [ ] A single `export_filter_directive(level)` function is called from `init`, `init_capsule`, and `test_layers` (grep: `rg -n "export_filter_directive" crates/jackin-diagnostics/src` → 4+ hits: 1 def, 3 calls)
- [ ] No production behavior change: `git diff` shows only `Cargo.toml` dev-deps, `#[cfg(test)]` items, the directive-string extraction, and tests
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- `opentelemetry_sdk` 0.32 as pinned does not expose in-memory exporters under any feature (check the lockfile version's docs first).
- Extracting the directive string produces a different string than either original (byte-compare in a unit test if unsure).
- The scoped-subscriber approach (`with_default`) cannot drive `JackinDiagnosticsLayer` + OTel layers together (e.g. registry trait bounds fail) after one reasonable fix attempt.
- Test 6 (`dependency_targets_pass_the_filter_today`) FAILS — that means the filter already changed and plan 002 may be partially landed; reconcile with `plans/README.md` before proceeding.

## Maintenance notes

- Plans 002, 003, 004, 006, 007 each deliberately flip specific tests written here; the plan text names them. A reviewer seeing one of these tests change outside those plans should treat it as an unintended export-contract change.
- The `test_layers` helper is the seam every future export-shape test should use; do not build a second one.
- Deferred (not this plan): a failing-exporter seam to test `record_otlp_internal` / flush-failure notices (audit finding; candidate follow-up after plan 004).
