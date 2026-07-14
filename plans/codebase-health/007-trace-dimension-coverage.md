# Plan 007: Trace & dimension coverage — stage span registry, screen.name everywhere, missing metrics, provider-identity decision

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-diagnostics/src/run.rs crates/jackin-diagnostics/src/screen.rs crates/jackin-diagnostics/src/metrics.rs crates/jackin-diagnostics/src/observability.rs`
> Mismatch with "Current state" = STOP. Requires plan 001 (registry).

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/codebase-health/001-telemetry-event-registry.md
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Four contract deltas from roadmap Telemetry item 4 and the span/metric contracts: (a) span names must be stable and registered — today `launch_stage_span` mints `launch.{stage}` from free caller strings, so a renamed stage silently creates a new span name; (b) `jackin.screen.name` must be stamped "on every log, span, and interaction metric produced under real console and capsule interaction" — today it exists only on screen spans; (c) the named database-statement and Docker-inspect metrics are required but absent, and the metric registry pins name/kind/unit but not aggregation or allowed dimensions; (d) the wire contract prohibits agent/provider identity in generic operational telemetry, yet `jackin.provider`/`jackin.agent.selected`/`jackin.agents.active` are stamped on generic screen/launch/capsule spans — while item 4 simultaneously wants "registered feature-decision events carrying key, provider, and bounded variant". The resolution the roadmap points to: provider/agent identity moves out of default span attributes into registered feature-decision events.

## Current state

- Dynamic stage spans, `crates/jackin-diagnostics/src/run.rs:1165-1187`:

```rust
fn launch_stage_span(stage: &str) -> tracing::Span {
    let otel_name = format!("launch.{}", normalize_stage_name(stage));
    let span = tracing::info_span!("launch_stage", stage = stage, otel.name = otel_name.as_str(), ...);
```

  (`normalize_stage_name` at `run.rs:1189` only lowercases/dots — no registry constraint. The `"derived image"` special case adds a span link — preserve it.)
- `jackin.screen.name` set once on the screen span: `crates/jackin-diagnostics/src/screen.rs:129` region; `record_capsule_activity` (`screen.rs:244-259`) stamps session/tab but no screen name; metrics recorders pass empty dims (`metrics.rs` — `record_frame`/`record_render`/`incr_mouse_events` with `&[]`).
- Metric registry: `observability.rs:81-108` (`otel_metrics::ALL`) + `metrics.rs:15-63` — terminal bytes, render duration/frames/cells, mouse, usage refreshes, typed errors. NO `docker.inspect` counter, NO db-statement counter, no aggregation/allowed-dims declaration, no exemplars anywhere.
- Provider/agent identity on generic spans: `screen.rs:174-187` (`set_agent_selected`, `set_agents_active`, `set_provider`), `screen.rs:213-233` (`launch_trace` stamps agent+provider), `screen.rs:253` (`record_capsule_activity` stamps `jackin.agent.selected`); keys at `observability.rs:48-51`.
- Docker-inspect choke point: `crates/jackin-docker/src/shell_runner.rs` / the docker client — find with `grep -rn "inspect" crates/jackin-docker/src --include='*.rs' | head`. DB statements: `crates/jackin-usage/src/telemetry_store.rs` (Turso statements; `upsert_account_snapshot_rows` at `:278`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Diagnostics | `cargo nextest run -p jackin-diagnostics --all-features` | pass |
| Docker/usage | `cargo nextest run -p jackin-docker -p jackin-usage --all-features` | pass |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Cross-crate | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-diagnostics/src/{run.rs,screen.rs,metrics.rs,observability.rs,registry.rs}` + sibling tests; the one Docker-inspect choke point in `crates/jackin-docker`; the DB-statement counter call in `crates/jackin-usage/src/telemetry_store.rs`; feature-decision event emission at the launch/provider-resolution site that currently calls `set_provider` (trace callers with `grep -rn "set_provider\|set_agent_selected\|set_agents_active" crates --include='*.rs' | grep -v diagnostics`).

**Out of scope**: HTTP client spans and failure-path migration (plan 008); the conformance matrix (009); any GenAI/consent telemetry design (explicitly future, per contract).

## Git workflow

Branch `feat/telemetry-trace-dimensions`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Registered launch stages

Introduce a closed `LaunchStage` enum (variants from the observed stage strings: run `grep -rn '\.stage(' crates --include='*.rs' | grep -v tests | head -30` and enumerate; expect preflight/image/materialization/run/attach/cleanup-family names plus `derived image`). `launch_stage_span(stage: LaunchStage)` uses `stage.span_name()` returning `&'static str` (e.g. `launch.derived_image`); keep the raw label as the bounded `jackin.stage` attribute. Update `run.stage(...)` callers; unknown strings no longer compile. Preserve the derived-image span-link block.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-runtime` → pass; `grep -n 'format!("launch.' crates/jackin-diagnostics/src/run.rs` → no matches.

### Step 2: screen.name on logs and interaction metrics

Expose the current screen name from `screen.rs` (thread-local it already keeps for the span — read `enter_screen`/`ScreenGuard`); stamp `jackin.screen.name` in the record-emit path (registry-validated optional attr) and pass it as a metric dimension for interaction/render metrics (`record_frame`, `record_render`, `incr_mouse_events`) — the screen set is a bounded enum so cardinality is safe. `record_capsule_activity` gains the capsule screen/tab name equivalent.

**Verify**: exporter-backed test asserts a log record and an interaction metric captured under an entered screen carry `jackin.screen.name`.

### Step 3: Missing metrics + registry completeness

Register `jackin.docker.inspect.count` (counter, unit `{inspection}`) and `jackin.db.statement.count` (counter, unit `{statement}`, dimension `statement.name` from a registered const set — never SQL text) in `otel_metrics` + `metrics.rs`; wire the docker-inspect choke point and `telemetry_store` statement execution. Extend the metric registry entries with declared allowed-dimension lists and aggregation (default sum/histogram as appropriate) — a static table validated by a test that every recorder call site uses only declared dims. Do NOT add run/session/container/model ids as dimensions (contract prohibition). Evaluate exemplars: if the pinned opentelemetry_sdk 0.32 supports exemplars on counters/histograms, attach trace context where available; if not, record "exemplars unsupported at SDK 0.32" in the module `//!` and skip.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-docker -p jackin-usage --all-features` → pass, incl. new metric emission tests.

### Step 4: Provider/agent identity → feature-decision events

Add registered feature-decision events to the registry (e.g. `feature.provider.selected` with attrs `feature.key`, `feature.provider`, `feature.variant` — bounded const sets). Change `set_provider`/`set_agent_selected`/`set_agents_active`/`launch_trace`/`record_capsule_activity` so generic spans no longer carry `jackin.provider`/`jackin.agent.*` as default attributes; the same call sites emit the registered feature-decision event instead. Add a negative exporter test: no generic span/record carries a provider/agent/model key.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → pass; negative sweep green.

### Step 5: Gates

`cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`; `cargo xtask ci --fast` → all exit 0.

## Test plan

Stage-enum compile coverage + span-name test; screen.name log/metric assertions; two new metric tests; feature-decision event test + provider-absence negative sweep. All exporter-backed, modeled on `observability/otlp/tests.rs` captures.

## Done criteria

- [ ] No `format!("launch.` span-name construction; stages are a closed enum
- [ ] Log + interaction-metric captures carry `jackin.screen.name` (host and capsule-tab)
- [ ] `jackin.docker.inspect.count` and `jackin.db.statement.count` registered and emitted
- [ ] Generic spans/records carry no provider/agent identity; feature-decision events carry it (tests prove both)
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Drift vs excerpts.
- Stage census reveals stages minted from user/config-controlled strings (unbounded) — enumerate; a closed enum may need a design call.
- Dashboards/queries documented in `docs/content/docs/reference/` depend on `jackin.provider` as a span attribute — surface before removing.
- Metric dimension addition trips the export-volume budget tests — reconcile with plan 009 rather than raising constants silently.

## Maintenance notes

- New stages = new enum variant + registry entry; reviewers reject raw-string stages.
- The feature-decision event registry is the only sanctioned carrier of provider identity in operational telemetry; a future GenAI schema is a separate consent-reviewed design (contract text).
