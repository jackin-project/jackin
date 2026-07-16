# Plan 004: The governed telemetry facade — typed events, operation guard, metrics registry, privacy limits, lint gates

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-telemetry crates/jackin-diagnostics/src/operation.rs crates/jackin-diagnostics/src/registry.rs crates/jackin-diagnostics/src/metrics.rs crates/jackin-diagnostics/src/redact.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/unified-otel-observability/001-telemetry-schema-crate.md, 002-otlp-composition-root.md, 003-tracing-bridge-layering.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the facade half of "Rust instrumentation architecture", the "Metrics" section, and "Privacy and limits"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

Product modules must emit through thin typed helpers in lowest-tier `jackin-telemetry` that expand to `tracing`, with fail-closed validation: registered names only, typed fields, allowlist-first privacy, hard size/cardinality ceilings, and typed rejection health instead of raw fallback. Metrics must be registered instruments with views, cumulative temporality, fixed boundaries, and a 256-attribute-set cap per stream. Today the closest equivalents live in `jackin-diagnostics` (`registry.rs` EventDef table, `operation.rs` guard, `metrics.rs` ad-hoc `jackin.*` instruments) and none enforce the roadmap ceilings. This plan builds the permanent API every call-site migration (plans 007–011) targets.

## Current state

(verified at planning commit)

- `crates/jackin-diagnostics/src/registry.rs` — fail-closed `EventDef` static table (26 events, `:406-857`), `validate(name, attrs, body)` (`:874`), `Outcome` (6 variants, `:32`), `PROHIBITED_EXPORT_KEYS: &["error_type","log.category","stage","kind","run_id"]` (`:180-181`). This is the philosophical ancestor — the new schema-driven registry replaces its string table.
- `crates/jackin-diagnostics/src/operation.rs:76-140` — `OperationGuard { span: Span, completed: AtomicBool }`; `complete(outcome, error_type)`; **Drop-without-complete records `Cancelled`** (`:124-129`); `Failure|Timeout` set span error status (`:113-115`). Guard is deliberately `Send` (holds `Span`, not `EnteredSpan`) — keep this property.
- `crates/jackin-diagnostics/src/metrics.rs` — `HotPathMetrics` (11 instruments, `jackin.*` names, `:78-90`), installed once via `install_hot_path(meter)` (`:15`); recorder fns no-op without provider. `crates/jackin-diagnostics/src/observability.rs:73-116` — `otel_metrics` name consts (all `jackin.*` or `tokio.runtime.*`/`process.*`).
- `crates/jackin-diagnostics/src/redact.rs` + `secret_scrub.rs` — regex redaction + byte caps; reuse, do not duplicate.
- Roadmap ceilings ("Privacy and limits"): 32 attrs/log, 64 attrs + 8 links/span, 32 attrs/metric point, 4 KiB redacted body/exception message, 1 KiB per string attribute, 32 array elements, 128 bytes per event/span/instrument name; redact-then-truncate; invalid anything ⇒ reject signal + typed health, never raw fallback.
- Roadmap metric families ("Metrics" table): CLI invocation count/duration/failure; UI transition/action/dwell/focus/render; launch stage duration/cache reuse; detached prewarm count/active/duration; background cycle count/duration; connection & RPC count/active/duration; agent-state transitions/stuck/flap; PTY/terminal/render throughput (with `stream.direction` only where applicable); process/runtime health (standard process + Tokio instruments); telemetry facade health (`telemetry.rejection.reason`, `telemetry.signal`). 30 s export interval (plan 002). Observable callbacks read atomics/cheap snapshots only.
- Lint machinery: no telemetry lint exists today. `cargo xtask lint --strict` runs arch/gates (`crates/jackin-xtask/src/arch.rs:123-129`); plan 001 added the `telemetry-registry` lane — extend it here.
- `#[tracing::instrument]` production sites to bring into policy: `crates/jackin-instance/src/lib.rs:420,448,627` (already `skip_all`), `crates/jackin-usage/src/usage.rs:423`, `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs:349`.

## Target API surface (in `crates/jackin-telemetry`)

```rust
// events
pub fn emit_event(def: &'static EventDef2, fields: FieldSet) -> Result<(), Rejection>;
// span/operation ownership
pub struct OperationGuard2 { /* Span + completion + schema-checked outcome */ }
pub fn operation(def: &'static SpanDef, attrs: AttrSet) -> OperationGuard2;
// metrics
pub struct Counter(&'static InstrumentDef); pub struct Histogram(&'static InstrumentDef); // typed handles
pub fn counter(def: &'static InstrumentDef) -> Counter; // registered-only construction
// health
pub fn facade_health() -> FacadeHealth;
```

Exact naming is the executor's choice within these rules: definitions come only from `jackin_telemetry::schema` (generated, plan 001); no `&str` name parameter anywhere on the public API; every emit path validates BEFORE formatting/allocation when telemetry is disabled (check the enabled flag first — the roadmap requires the disabled fast path to avoid formatting/allocation, benchmarked in plan 014). The `outcome` vocabulary is the schema enum: `success | failure | error | timeout | skip | cancellation` — note this is richer than the current `Outcome` (which lacks the failure-vs-error split); map old call sites during plans 007–011, not here.

`jackin-telemetry` does NOT dispatch to providers — it expands to `tracing` (events/spans) and to `opentelemetry` Meter API instruments handed to it by `jackin-diagnostics` at init (an `install(meter)` hook mirroring today's `install_hot_path` pattern). It is not another logging framework.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-telemetry -p jackin-diagnostics --all-features --locked` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Lint lane | `cargo xtask telemetry-registry` | exit 0 |
| Full lint | `cargo xtask ci --only lint` | exit 0 |
| Bench (fast path, informal now) | `cargo bench -p jackin-telemetry -- --quick` | completes |

## Scope

**In scope:**
- `crates/jackin-telemetry/src/` — new modules: `event.rs`, `operation.rs`, `metric.rs`, `limits.rs`, `privacy.rs`, `health.rs` (+ sibling `tests.rs` each), `benches/disabled_fast_path.rs`
- `crates/jackin-diagnostics/src/` — wire the facade: install hook for metric instruments, application processor validating the allowlist again before export ("Application processors validate the allowlist again before export"), redact reuse, view registration (cumulative temporality, fixed histogram boundaries), the 256-attribute-set cap with reject-and-count (never evict)
- `crates/jackin-xtask/src/telemetry_registry.rs` — add lint checks (below)
- `ratchet.toml` — public-surface bound updates for jackin-telemetry; export-volume rows only if the facade-health metrics change default-mode counts
- `crates/jackin-telemetry/registry/metrics.yaml` — fill the instrument definitions for the roadmap metric families (names must be standard or from the neutral extension registry — e.g. `gen_ai.client.token.usage` is standard; PTY/render throughput instruments get neutral names like `terminal.io.bytes` with `stream.direction` attr — no `jackin.*` names)

**Out of scope:**
- Migrating any product call site (plans 007–011). Old `operation.rs`/`registry.rs`/`metrics.rs` in jackin-diagnostics keep working in parallel until then.
- Deleting `jackin.*` metric names (plan 013 removes the old instruments with their callers).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(telemetry): governed event/operation/metric facade with privacy limits`. Sign `-s`, push after every commit.

## Steps

### Step 1: Limits + privacy modules

`limits.rs`: the ceilings table as consts + `fn clamp_body(&str) -> Cow<str>` (redact FIRST via a caller-supplied redactor hook, then UTF-8-safe truncate to 4 KiB), `fn validate_attr(key, value) -> Result<(), Rejection>` (1 KiB strings, 32 array elems, name ≤ 128 bytes). `privacy.rs`: allowlist check — a key is valid iff it exists in the generated schema (standard import or extension); everything else ⇒ `Rejection::Privacy` or `Rejection::UnknownAttribute`. `Rejection` enum mirrors `telemetry.rejection.reason` schema values: `unknown_name | unknown_attribute | invalid_value | privacy | cardinality | size_limit`.

**Verify**: `cargo nextest run -p jackin-telemetry --locked -E 'test(limits) or test(privacy)'` → pass.

### Step 2: Event + operation API

`event.rs`: `emit_event` validating name/fields against schema defs, then `tracing::event!(name: <static>, target: TELEMETRY_TARGET, level, { … })`. Level comes from the def. Rejects update `health.rs` counters and increment the facade-health metric; the signal is dropped (no raw fallback). `operation.rs`: port the guard pattern from `jackin-diagnostics/src/operation.rs:76-140` (Send guard holding `Span`; drop-without-complete = `cancellation` **with status Unset** — per the contract, expected cancellation leaves status unset; `failure|error|timeout` set Error status + `error.type`; `skip` and `success` leave Unset). Include `link(&SpanContext)` support (≤ 8 links) and `set_attr` with validation. Async guidance in rustdoc: attach via `.instrument(guard.span().clone())`, never `.enter()` across `.await` (same doc contract as the old guard, `operation.rs:65-74`).

**Verify**: unit tests for each outcome→status mapping; guard-drop test asserting `cancellation` + Unset status.

### Step 3: Metrics registry + views + cardinality cap

`metric.rs`: typed handles constructed only from generated `InstrumentDef`s; an `install(meter: &opentelemetry::metrics::Meter)` entry point called by `jackin-diagnostics` after provider build (model on `install_hot_path`, `jackin-diagnostics/src/metrics.rs:15`); per-stream active-attribute-set tracking with the 256 cap — on overflow reject the new set, count it (`telemetry.rejection.reason=cardinality`), never evict a live series. In `jackin-diagnostics`: register views for fixed histogram boundaries + cumulative temporality; keep observable callbacks (process CPU/memory, Tokio runtime) reading atomics/cheap snapshots only — the existing `ProcSampler` throttle pattern (`observability.rs:1185-1213`) is the exemplar.

**Verify**: cardinality test — 257 distinct attr sets on one counter ⇒ 256 recorded, 1 rejection counted. Views test — histogram boundaries applied (in-memory metric exporter).

### Step 4: Facade health

`health.rs`: atomics per `(signal, reason)`; snapshot type feeding both the `telemetry.rejection.reason`/`telemetry.signal` metric family and plan 002's `TelemetryHealth`.

**Verify**: rejection increments visible in snapshot + exported metric (in-memory exporter test).

### Step 5: Second-line export validator

In `jackin-diagnostics`, add an application processor (span processor + log processor wrapper) that re-validates outgoing attributes against the allowlist and prohibited-value rules before export, dropping + counting violations. This is defense-in-depth behind the facade ("Application processors validate the allowlist again before export").

**Verify**: test — a span attribute injected around the facade (raw `tracing` with a bogus field on a governed target) does not reach the in-memory exporter; rejection counted.

### Step 6: Lint gates

Extend `cargo xtask telemetry-registry` with source-scan checks over production crates (exclude `jackin-xtask`, `jackin-dev`, `jackin-pr-trailers`, `jackin-tui-lookbook`, `jackin-lints`, `*/tests.rs`, `benches/`):
1. `#[tracing::instrument]`/`#[instrument]` without `skip_all` ⇒ fail (current sites at `jackin-instance/src/lib.rs:420,448,627` already comply; `jackin-usage/src/usage.rs:423` and `jackin-runtime/.../launch_pipeline.rs:349` — check and, if non-compliant, add to a shrink-only allowlist drained by plans 008/011 rather than fixing here).
2. Raw `opentelemetry::logs`/`Logger` use outside `jackin-diagnostics`+`jackin-telemetry` ⇒ fail.
3. `Meter::create_*`/instrument construction outside the two telemetry crates ⇒ fail (allowlist current `metrics.rs`/`observability.rs` until plan 013).
4. Raw `tracing::(event!|info!|warn!|error!|debug!|trace!|span!|info_span!|…)` in production crates outside the two telemetry crates ⇒ fail, seeded shrink-only allowlist from today's inventory (`jackin-runtime` 11 events, `jackin-instance` 3 + 3 `#[instrument]`, `jackin-usage` 1 span, `jackin-diagnostics` internals). Plans 007–011 drain it.
5. Formatter layers: `tracing_subscriber::fmt` outside `jackin-diagnostics` ⇒ fail.

**Verify**: `cargo xtask telemetry-registry` → exit 0 on clean tree; each rule trips on a synthetic violation (add, observe failure, revert).

### Step 7: Disabled fast-path benchmark scaffold

`crates/jackin-telemetry/benches/disabled_fast_path.rs` (criterion, `harness = false`, `[[bench]]` entry): benchmark `emit_event` + guard create/drop + counter add with telemetry disabled; assert (in the paired unit test, not the bench) zero formatting via a `Display`-impl probe that panics if invoked when disabled. Plan 014 wires thresholds; here it only has to exist and run.

**Verify**: `cargo bench -p jackin-telemetry -- --quick` completes; probe unit test passes.

## Reopened audit additions (2026-07-16)

- Generate non-forgeable per-event/span/instrument descriptors containing required/allowed attributes, wire types, bounded-enum validators, unit/description, and metric-dimension policy. Validate the entire initial signal before constructing/emitting it; a rejected operation exports zero spans.
- Redact then UTF-8 truncate event bodies, exception fields, and status descriptions inside the facade. Operation completion reserves capacity and validates outcome plus a generated stable error type rather than bypassing limits.
- Outcome tests must distinguish: expected cancellation (Unset), deadline/dependency cancellation (Error with stable `error.type`), and a guard abandoned without an explicit outcome (instrumentation fault). Recovered degradation is one governed WARN carrying fixed bounded `error.type=recovered_degradation` without a body, and Plan 011 must prove no duplicate ERROR narration remains.
- The second-line validator covers metric points as well as logs and spans, including raw/bypassed metric-point privacy, key, value, count, size, and cardinality negatives.
- Facade health is the full `(telemetry.signal, telemetry.rejection.reason)` matrix and exports through a nonallocating observable instrument. Every rejection class must be reachable through a real governed path.
- Construct registered metric handles once during installation. Canonicalize dimension sets by schema identity without formatting/allocation, reject duplicate keys, and prove order-independent cardinality plus exact 256-series export behavior.
- Observable callback enforcement must structurally and dynamically reject I/O, async locks, filesystem scans, and runtime entry; callbacks may read only atomics or cheap synchronous snapshots.
- Delete or govern every public string-name span/metric API in diagnostics, and make the source-policy lint syntax-aware with permanent allowed/prohibited fixtures.
- Provide one generic `ResultTelemetryExt` ownership-boundary helper that records any `Err` as the registered `error.typed` event without formatting the error value. Automatic `tracing-opentelemetry` inference remains disabled: the semantic operation owner explicitly completes span status, preventing handled inner errors from poisoning successful outer operations or exporting raw error text.

## Test plan

- Per-module `tests.rs` files as written into the steps (limits, privacy, outcome mapping, cardinality, views, health, second-line validator, disabled-probe).
- Model test style on `crates/jackin-diagnostics/src/registry/tests.rs` (fail-closed assertions) and `observability/otlp/tests.rs` (in-memory export assertions).
- All tests must run under `cargo nextest run --workspace --all-features --locked`.

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] `cargo xtask telemetry-registry` exits 0 and fails on each synthetic violation class (spot-checked)
- [ ] `grep -rn "pub fn .*(&str" crates/jackin-telemetry/src/ | grep -v tests` shows no name-by-string public emit API
- [ ] Cardinality cap test (256/reject) passes
- [ ] `cargo bench -p jackin-telemetry -- --quick` completes
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- The 0.32 Meter API cannot express reject-without-evict cardinality capping at the facade layer (e.g. views force eviction semantics) — describe the SDK behavior observed.
- The second-line processor cannot see log-record attributes pre-export in the 0.32 SDK.
- Lint rule 4's seeded allowlist would exceed ~40 entries (inventory drift — re-run the census and report).

## Maintenance notes

- Plans 007–011 migrate every product call site onto this API; its ergonomics get locked in quickly — a reviewer should sanity-check that a two-line "event with three fields" emit stays two lines.
- The `failure` vs `error` outcome split (domain-negative vs infrastructure fault) is new judgment at each call site; the migration plans carry a decision rule — the facade only enforces the vocabulary.
- Facade-health metrics feed `jackin diagnostics validate` (plan 012) — keep `FacadeHealth` additive.
