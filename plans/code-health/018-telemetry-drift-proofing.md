# Plan 018: Phase 8 — telemetry drift-proofing: one OTLP builder, full semconv registry, correlatable file sinks, honest failure surfaces

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-diagnostics/src/run.rs crates/jackin-usage/src/logging.rs crates/jackin-runtime/src/runtime/launch/failure.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M-L
- **Risk**: MED (touches live telemetry init; the in-memory exporter tests are the safety net)
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `47dd5fca0`, 2026-07-09
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`, 2026-07-11)

## Why this matters

The Phase 8 audit found the code materially ahead of the roadmap prose — error semantics (`error.type`, span status, outcome taxonomy) and the run-id contract (wrapper-injected id wins, JSONL as fallback sink) are largely shipped — but four drift traps remain, each cheap to close and each compounding if left: (1) host and capsule OTLP layer construction are two hand-synced copies whose own doc comment warns "a change to any of that setup must touch both"; (2) the semconv registry (`otel_keys`) centralizes attribute names only — metric names and event/kind names are inline string literals, so a renamed metric drifts silently; (3) the JSONL diagnostics file claims `trace_id`/`span_id` but writes the run id and a tracing-runtime u64 — not the OTel hex ids the backend ships, so a fallback file cannot be joined to its trace; the capsule's `multiplexer.log` carries no run/session context at all; (4) failure surfaces print the run id but never the backend query, so an operator gets an id they must know what to do with. This plan also reconciles the roadmap text with what already shipped, per the audit's P8-09 finding.

## Current state

All excerpts verified by direct read at `47dd5fca0`.

- `crates/jackin-diagnostics/src/observability.rs`:
  - `otlp::init` (host, line ~670) and `otlp::init_capsule` (line ~768) each build: `ensure_grpc_protocol()`, `otel_runtime()?.enter()` guard, `SpanExporter::builder().with_tonic().with_endpoint(…)`, `LogExporter::builder()…`, `SdkTracerProvider::builder().with_span_processor(BatchSpanProcessor::builder(exporter, Tokio).build()).with_resource(…)`, same for logger, then `init_metrics(&resource, …)`. The `init_capsule` doc comment states: "The shared preamble (`ensure_grpc_protocol`, the dedicated `otel_runtime().enter()` guard, the `with_tonic()` exporter and Batch-processor builds) duplicates `init` because the layer composition differs structurally; a change to any of that setup must touch both." Differences between the two: resource (`build_resource(run_id)` line ~660 vs `capsule_resource(session_id, run_id)` line ~749), layer composition (host adds `JackinDiagnosticsLayer`, capsule adds `otlp_diag_layer`), and the capsule's endpoint normalization (`grpc_endpoint(endpoint)`).
  - `otel_keys` module (lines 23-55): attribute-name constants only — `SERVICE_NAME`, `SESSION_ID`, `RUN_ID = "parallax.run.id"`, `COMPONENT`, `SCREEN_NAME = "jackin.screen.name"`, `SCREEN_FROM`, `WORKSPACE`, … Metric names are literals inside `init_metrics` (~line 1043: `"process.cpu.utilization"`, `"tokio.runtime.workers"`, `"jackin.diagnostics.events"`, `"jackin.cache.hits"`, `"jackin.cache.misses"`, …); event `kind` → name mapping is string-munging in `event_taxonomy`/`operation_for`/`category_for` (~lines 1263-1308) with literal kinds (`"stage_started"`, `"otlp_internal"`, `"run_summary"`, …).
  - `span_id` for JSONL comes from `tracing::Span::current().id().map(|id| id.into_u64().to_string())` (~line 1377) — the tracing registry's u64, not the OTel hex span id.
  - In-memory test rig exists: `TestExport`/`test_layers` (~lines 911-943) with `InMemorySpanExporter`/`InMemoryLogExporter`, exercised by `observability/otlp/tests.rs` (resource attrs, error.type, span status assertions).
- `crates/jackin-diagnostics/src/run.rs`: `JsonEvent` construction (~line 816-830) sets `run_id: &self.run_id, trace_id: &self.run_id, span_id, …` — **trace_id is the run id**. Outcome taxonomy `outcome_for` (~line 1310) keys off substrings (`failed`/`failure`/`crash`/`error_type`); no typed expected-shutdown outcome. Host panic hook at ~line 917-928 (`run.error_typed("panic", …)`).
- `crates/jackin-usage/src/logging.rs`: `write_line` (~line 173) stamps `"{ts} {message}"` — timestamp + `[jackin-capsule]` prefix only; its doc comment says the file must be reconstructable alone, yet it carries no run/session/trace context. `clog!` macro (~line 195) formats, `write_line`s, and `telemetry::bridge_log`s.
- `crates/jackin-runtime/src/runtime/launch/failure.rs` (~line 68): failure report builds a table `[["run id", run.run_id()], ["run diagnostics", path], ["docker output", path]]` — no backend query line. (Other failure surfaces exist; this is the exemplar — Step 5 greps for siblings.)
- `JACKIN_TELEMETRY_LEVEL` already exists as the level knob (TESTING.md:63 documents `JACKIN_TELEMETRY_LEVEL=trace`); per-sink filters do NOT (host `init` ~line 727 applies one `EnvFilter` directive clone to both span and log layers; same in capsule ~line 821) — per-sink filtering is recorded next-wave, not this plan.
- Conventions: `jackin-diagnostics` is the telemetry home; two-tier macro rules in ENGINEERING.md; comments = non-obvious WHY only.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests (the safety net) | `cargo nextest run -p jackin-diagnostics` | all pass |
| Capsule + usage tests | `cargo nextest run -p jackin-usage -p jackin-capsule` | all pass |
| Runtime tests | `cargo nextest run -p jackin-runtime` | all pass |
| Workspace clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-diagnostics/src/observability.rs` (+ its tests) — shared builder, semconv extension, real span ids
- `crates/jackin-diagnostics/src/run.rs` (+ tests) — trace-id honesty, ExpectedShutdown outcome
- `crates/jackin-usage/src/logging.rs` (+ tests) — capsule log context banner
- `crates/jackin-runtime/src/runtime/launch/failure.rs` and sibling failure surfaces found in Step 5
- Roadmap Phase 8 section prose reconciliation
- READMEs of touched crates if module structure changes (it should not)

**Out of scope** (recorded next wave):
- The typed operation facade (`operation_span/log/error/metric`) and macro-stack unification — L, multi-phase, the dossier's Phase 1
- Per-sink filters / retiring `JACKIN_DEBUG` as a knob
- The 9-instrument metric set (terminal bytes, painted cells, DB counters)
- The launch-conformance test lane + export-volume ratchet
- Collapsing the duplicate `debug_log!` definitions (jackin-core vs jackin-diagnostics)

## Git workflow

- Branch off `main`: `refactor/telemetry-drift-proofing`.
- Conventional Commits, `-s`, push per commit. PR to `main`; do not merge. This touches the capsule dependency closure, so the PR body needs the capsule smoke block per `.github/PULL_REQUEST_TEMPLATE.md` — copy it verbatim.

## Steps

### Step 1: One OTLP provider builder

In `observability.rs`, extract the duplicated preamble into one function, e.g. `fn build_otlp_providers(resource: Resource, traces_endpoint: &str, logs_endpoint: &str) -> Result<(SdkTracerProvider, SdkLoggerProvider, RuntimeGuardParts…)>` — exact signature driven by what the two callers share (read both functions fully first; the shared set is: grpc-protocol check, runtime guard, both exporters, both providers with batch processors). `init` and `init_capsule` shrink to: build their resource, call the builder, compose their differing layers (`JackinDiagnosticsLayer` vs `otlp_diag_layer`), install. Delete the "must touch both" comment — the point of the change. Preserve observable behavior bit-for-bit: same endpoints, same batch config, same resource attributes (the existing `otlp/tests.rs` resource/endpoint assertions must pass unchanged).

**Verify**: `cargo nextest run -p jackin-diagnostics` → all pass, zero test edits in this step.

### Step 2: Extend the semconv registry to metrics and events

In `otel_keys` (or sibling modules `otel_metrics`/`otel_events` next to it — prefer submodule consts in the same file to keep one registry), add constants for every metric name currently literal in `init_metrics` and every event `kind`/`event.name` literal in `event_taxonomy`/`operation_for`/`category_for`. Replace the literals with the constants. Add a test asserting registry completeness the cheap way: a unit test listing the const values and asserting `init_metrics`' instruments and `event_taxonomy`'s match arms use them (if the existing tests already snapshot metric names, extend those). Do not rename any wire name — this step centralizes, never changes, emitted strings (renames would orphan backend history).

**Verify**: `cargo nextest run -p jackin-diagnostics` → pass; `rg '"process\.cpu\.utilization"|"jackin\.cache\.hits"' crates/jackin-diagnostics/src/observability.rs` → matches only inside the const definitions.

### Step 3: Real trace/span ids in the JSONL; run context in the capsule log

1. JSONL: where `JsonEvent` is built (`run.rs` ~816-830), populate `trace_id`/`span_id` from the active OTel span context when OTLP is installed: via `tracing_opentelemetry::OpenTelemetrySpanExt`-style lookup (the workspace already depends on the tracing-opentelemetry bridge — find the existing import in observability.rs; if the span has a valid `SpanContext`, use its 32-hex trace id and 16-hex span id). When no OTLP is active (file-only mode), keep today's values but rename honestly: keep `trace_id` = run id ONLY if the field must stay non-empty for schema stability — otherwise emit the field only when real. Decide by reading the JSONL consumers (`crates/jackin-diagnostics/src/summary.rs` and `crates/jackin/src/cli/diagnostics.rs` — grep for `trace_id`); if any consumer requires non-empty, keep the fallback and add a `trace_id_source: "otel"|"run-id"` field is NOT wanted (schema noise) — instead document the fallback in the field's doc comment. If consumers do not read it, prefer empty-when-absent. Report which branch you took in the PR body.
2. Capsule log: in `logging.rs`, at log-file open (find where `LOG_FILE` is initialized), write one banner line: `[jackin-capsule] context run_id=<id> session_id=<id> traceparent=<w3c or "-">` from the values the capsule already receives (`init_capsule_tracing` in `crates/jackin-usage/src/telemetry.rs:23` gets session_id/run_id/traceparent — thread them or read from the same source). Per-line stamping is deliberately out (volume); one banner makes the file joinable offline.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-usage` → pass; extend `otlp/tests.rs` with one test: with test_layers installed and a span active, an emitted JSONL event's `trace_id` equals the in-memory exporter's span trace id (32 hex chars).

### Step 4: Typed expected-shutdown outcome

In `run.rs`'s outcome taxonomy (`outcome_for` ~1310): add an explicit typed path so expected shutdowns/detaches are never failure-shaped. Concretely: introduce `pub(crate) enum EventOutcome { …existing…, ExpectedShutdown }` or (if outcomes are strings) a recognized `"expected_shutdown"` outcome that callers select via a new `kind` (e.g. `"session_detach"`, `"clean_shutdown"`) mapped explicitly in the taxonomy, bypassing the `failed`/`failure` substring sniffing. Find the emit sites for clean detach/shutdown (grep `detach|shutdown` in `crates/jackin-runtime/src/runtime/` and `crates/jackin/src/`) and route the 1-3 clearest ones through it. Keep the substring path for everything else (full migration is facade-wave work).

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-runtime` → pass; new unit test: a `session_detach` event yields outcome `expected_shutdown`, not `failure`.

### Step 5: Backend query on failure surfaces

In `failure.rs`'s report table, after the "run diagnostics" row, add a row `["backend query", …]` populated when an OTLP endpoint is configured: the diagnostics crate knows the endpoint (`configured_endpoint_summary` exists per the audit — grep observability.rs for it); render `parallax run <run-id>` when the endpoint summary indicates Parallax conventions are in play, else the neutral `query your OTLP backend for parallax.run.id=<run-id>`. When no endpoint is configured, keep today's rows (the JSONL path row already serves). Then `rg -l 'run id' crates/jackin-runtime/src crates/jackin/src` for sibling failure/teardown surfaces printing a bare run id and apply the same one-line addition where the run id is already in hand (expect 2-4 sites; list them in the PR body).

**Verify**: `cargo nextest run -p jackin-runtime` → pass (failure.rs has table-shape tests if any exist — run the crate suite either way); `cargo clippy -p jackin-runtime --all-targets -- -D warnings` → exit 0.

### Step 6: Roadmap reconciliation

Rewrite roadmap Phase 8 items 7 and 9 to match shipped reality (per audit P8-09): error semantics (error.type + span status + outcome taxonomy + fingerprint-stable fields) and the run-id contract (wrapper id wins, JSONL fallback-only) are **largely shipped** — the residuals were exactly this plan's Steps 4-5. Mark items 1 (facade), 2 (per-sink filters), 6 (metric set), 8 (conformance lane + volume budget) as the remaining open work. Update item 3's status: shared builder + real JSONL ids shipped here. Note the `jackin.screen.name` vs `app.screen.name` naming question as a recorded decision: the code's `jackin.`-namespaced key is canonical (do not rename wire attributes).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Existing `observability/otlp/tests.rs` suite green with zero assertion changes through Step 1 (that is the refactor's proof).
- New: JSONL-trace-id-matches-exporter test (Step 3), expected-shutdown outcome test (Step 4), registry-completeness test (Step 2).
- Full: `cargo nextest run -p jackin-diagnostics -p jackin-usage -p jackin-runtime -p jackin-capsule`; workspace clippy.

## Done criteria

- [ ] One provider-builder function; `init`/`init_capsule` contain no duplicated exporter/provider construction; the "must touch both" comment gone
- [ ] No inline metric/event name literals outside the registry consts (`rg` check from Step 2)
- [ ] JSONL `trace_id`/`span_id` carry real OTel hex ids when OTLP is active; capsule log opens with the context banner
- [ ] Expected shutdown/detach events produce a typed non-failure outcome
- [ ] Failure surfaces print a backend query line when an endpoint is configured
- [ ] Roadmap Phase 8 text matches shipped reality
- [ ] All four crate suites + workspace clippy green; `cargo xtask ci --fast` → `ci gate OK`
- [ ] `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- The two init functions' "shared preamble" turns out to differ in any parameter beyond resource/endpoint/layers (e.g. different batch configs) — a silent unification would change export behavior; report the diff instead.
- A JSONL consumer (summary.rs / cli/diagnostics.rs) depends on `trace_id == run_id` (Step 3's read) — then only the capsule banner + span_id fix land, and the trace_id change is reported as blocked.
- `tracing-opentelemetry` (or equivalent span-context access) is not actually among the dependencies — do not add a new dependency without reporting first.
- Any wire attribute/metric/event NAME would need to change to complete a step — never rename emitted names in this plan.

## Maintenance notes

- The facade wave (recorded next-wave) builds directly on Steps 1-2: `operation_metric` will mint from the registry consts, and per-sink filters slot into the single builder.
- The conformance lane (dossier acceptance checks L1090-1106) should assert Step 3's ids end-to-end when it lands.
- Reviewer should scrutinize: byte-for-byte parity of exporter/batch configuration through the Step 1 refactor, and that Step 5's backend-query line renders sanely when the endpoint summary is a long URL (table width).
