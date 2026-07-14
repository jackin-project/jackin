# Plan 001: Typed event registry + canonical attribute schema in jackin-diagnostics

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/codebase-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-diagnostics/ crates/jackin-usage/src/telemetry.rs`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: none
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

The roadmap's "OTLP LogRecord wire contract" (in `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx`, section "Telemetry convergence") requires every registered event definition to carry: top-level name; default severity; one stable body intent; required and optional attributes with native types; allowed outcomes; privacy class; cardinality class; sink eligibility; error-fingerprint inputs; and the owning crate — with registration failing closed on unknown keys, missing required keys, prohibited spellings, or a body that violates its redaction/size policy. Today none of that exists: event names are derived at runtime with `kind.replace('_', ".")` (a shape the contract explicitly prohibits), outcomes/categories/components are computed by substring sniffing, the failure-grouping key ships as `error_type` (prohibited spelling — the contract requires `error.type`), the stage dimension ships as bare `stage` instead of `jackin.stage`, expected shutdown ships the unregistered outcome `expected_shutdown` instead of `expected_close`, and the typed facade silently discards every attribute callers pass it. This plan builds the registry and fixes the canonical spellings; plans 002–009 all validate against it.

## Current state

Files (all under `crates/jackin-diagnostics/src/` unless noted):

- `observability.rs` (~1750 lines) — OTLP init, `otel_keys`/`otel_events`/`otel_metrics` const modules, the runtime `EventTaxonomy`, and the JSONL/OTLP emit macro.
- `operation.rs` (~207 lines) — the typed operation facade (`operation_span`, `operation_log`, `operation_error`, `operation_metric`).
- `observability/tests.rs`, `observability/otlp/tests.rs`, `operation/tests.rs` — exporter-backed tests (they use `InMemoryLogExporter`/`InMemorySpanExporter`; several currently assert the *non-conforming* shape and must be flipped, not deleted).
- `crates/jackin-usage/src/telemetry.rs` — capsule bridge (only referenced here; migrated in plan 004).

The "registry" today is a flat const list, `observability.rs:112`:

```rust
pub mod otel_events {
    pub const STAGE_STARTED: &str = "stage_started";
    pub const STAGE_DONE: &str = "stage_done";
    ...
    pub const PROCESS_EXECUTE: &str = "process.execute";
    pub const ALL: &[&str] = &[ ... ];
}
```

Runtime taxonomy derivation, `observability.rs:1402-1418` (the prohibited `kind.replace` is line 1410):

```rust
pub(crate) fn event_taxonomy(kind: &str, message: &str, stage: Option<&str>, detail: Option<&str>, error_type: Option<&str>, level: &str) -> EventTaxonomy {
    let event_name = kind.replace('_', ".");
    EventTaxonomy {
        operation: operation_for(kind, stage, &event_name),
        category: category_for(kind, stage, detail),
        outcome: outcome_for(kind, error_type, level),
        component: component_for(kind, message),
        event_name,
    }
}
```

`outcome_for` (`observability.rs:1461-1484`) returns `"expected_shutdown"` for `SESSION_DETACH | CLEAN_SHUTDOWN` and otherwise sniffs substrings (`kind.contains("failed")` etc.). `component_for` (`observability.rs:1486-1492`) sniffs `message.starts_with("[jackin-capsule")`.

The emit macro (`observability.rs:1623` onward, `emit_jsonl_event_fields!`) emits per-arm:

```rust
tracing::$emit!(
    target: JSONL_TARGET,
    run_id = $run_id,
    kind = $kind,
    event.name = $taxonomy.event_name.as_str(),
    event.outcome = $taxonomy.outcome,
    jackin.component = $taxonomy.component,
    jackin.operation = $taxonomy.operation.as_str(),
    jackin.category = $taxonomy.category.as_str(),
    stage = stage,          // ← must become jackin.stage
    detail = detail,        // ← non-canonical key
    error_type = error_type, // ← must become error.type
    "{}", $message
),
```

The typed facade drops caller attributes, `operation.rs:92-141` (note `let _ = attrs;` at line 101) and `operation_error` (`operation.rs:145-168`, `let _ = attrs;` at line 148) hardcodes `"event.name" = "error"`, `"jackin.category" = "error"`, and `error_type = error_type`. The `Warn` arm of `operation_log` also hardcodes `"event.outcome" = "success"`. `OperationGuard` (`operation.rs:57-77`) has no completion hook, so spans that end via `?` record no outcome.

Repo conventions to match: Rust 2024 self-named modules, no `mod.rs`; all tests for a module in a single sibling `tests.rs` (see `crates/AGENTS.md`); comments explain WHY only; workspace lints are strict (`-D warnings`, `unwrap_used`/`expect_used` denied — use `Result` flow or narrow `#[expect(..., reason = "…")]`).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-diagnostics --all-features` | all pass |
| Crate lint | `cargo clippy -p jackin-diagnostics --all-targets --all-features -- -D warnings` | exit 0 |
| Format | `cargo fmt` then `cargo fmt --check` | exit 0 |
| Cross-crate gate | `cargo xtask ci --fast` | exit 0 |
| Consumers | `cargo nextest run -p jackin-usage -p jackin-docker` | all pass |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-diagnostics/src/observability.rs` (+ new `observability/registry.rs` or sibling `registry.rs` module with its `registry/tests.rs`)
- `crates/jackin-diagnostics/src/operation.rs`, `operation/tests.rs`
- `crates/jackin-diagnostics/src/observability/tests.rs`, `observability/otlp/tests.rs`, `src/tests.rs`
- `crates/jackin-diagnostics/src/run.rs` only where it names `expected_shutdown`/`error_type` spellings
- `crates/jackin-diagnostics/README.md` (structure table gains the registry module)

**Out of scope** (do NOT touch, even though they look related):
- `crates/jackin-usage/src/logging.rs` and `telemetry.rs` — capsule bridge migration is plan 004.
- OTLP `Resource` construction (`build_resource`/`capsule_resource`) — plan 002.
- The tracing bridge / top-level `EventName` population — plan 003.
- JSONL file schema/adapter (`run.rs` `JsonEvent` serialization beyond the two spellings above) — plan 005.
- Any call-site migration outside jackin-diagnostics — plan 008.

## Git workflow

- Branch: `refactor/telemetry-event-registry` off `main` (propose to operator per repo rule if session policy requires confirm).
- Conventional Commits, DCO sign-off, push after every commit: `git commit -s -m "refactor(diagnostics): …" && git push`.
- Do not open the PR as draft-less until `cargo xtask ci --fast` is green.

## Steps

### Step 1: Introduce the registry module

Create `crates/jackin-diagnostics/src/registry.rs` (+ `registry/tests.rs`, declared from `lib.rs`). Define:

```rust
pub struct EventDef {
    pub name: &'static str,              // dotted, e.g. "docker.container.inspect"
    pub severity: Severity,              // default severity
    pub body: &'static str,              // one stable body intent, e.g. "container inspected"
    pub required: &'static [AttrDef],
    pub optional: &'static [AttrDef],
    pub outcomes: &'static [Outcome],    // allowed event.outcome values
    pub privacy: Privacy,                // e.g. Routine | Evidence
    pub cardinality: Cardinality,        // Low | Bounded
    pub sinks: SinkSet,                  // otlp / jsonl / capsule-file eligibility
    pub fingerprint: &'static [&'static str], // error-fingerprint attr inputs
    pub owner: &'static str,             // owning crate
}
```

`AttrDef` carries key + native type (`Str`/`I64`/`F64`/`Bool`/`StrArray`). `Outcome` is a closed enum: `Success`, `Failure`, `Timeout`, `Cancelled`, `CacheHit`, `ExpectedClose` (note: **`expected_close`**, not `expected_shutdown`). Seed the registry with one `EventDef` per existing `otel_events` kind (translate each snake_case kind to its dotted name once, statically — e.g. `stage_failed` → `launch.stage.failed`, `session_detach` → `capsule.session.detach`, keep `process.execute`). Add a `lookup(name) -> Option<&'static EventDef>` and a fail-closed `validate(name, attrs, body) -> Result<(), RegistryError>` that rejects unknown event names, unknown attribute keys, missing required keys, prohibited spellings (`error_type`, `log.category`, bare `stage`, `kind`, `run_id` as export keys), and bodies with bracket prefixes (`starts_with('[')`).

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(/registry::tests/)'` → new tests pass (write them in step 2).

### Step 2: Registry tests (fail-closed behavior)

In `registry/tests.rs` cover at minimum: unknown event name rejected; unknown attr key rejected; missing required attr rejected; `error_type`/`log.category`/`kind` keys rejected; `[jackin` body rejected; every seeded def has non-empty dotted name (contains `.`, no `_`), at least one allowed outcome, and an owner; `expected_shutdown` is not an allowed outcome anywhere.

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(/registry::tests/)'` → all pass.

### Step 3: Route taxonomy through the registry

Replace the body of `event_taxonomy` (`observability.rs:1402`) so it looks up the registry def for the kind (add a static `kind -> &EventDef` map beside the defs) and uses the def's dotted `name`, its default component/category/operation, and validates the outcome against `def.outcomes`. Delete `kind.replace('_', ".")`, `operation_for`'s `_ => event_name` fallback for unregistered kinds, and the substring arms of `outcome_for`/`component_for` that the registry now answers statically. Where genuinely dynamic input remains (stage-qualified operation like `stage.preflight`), keep the qualifier but validate the stage token against a registered stage list (introduce `otel_stages` consts: `preflight`, `image`, `run`, `attach`, `cleanup`, plus the stages observed in existing fixtures). Emit `expected_close` (never `expected_shutdown`) for `session_detach`/`clean_shutdown`.

**Verify**: `cargo nextest run -p jackin-diagnostics` → failures only in tests that assert the old derived names/outcomes; fix those tests to assert the registered names (do not weaken assertions — each must pin the exact new dotted name).

### Step 4: Canonical spellings in the emit macro

In `emit_jsonl_event_fields!` (`observability.rs:1623`) and the error/info emit fns: rename `stage = stage` → `jackin.stage = stage`, `error_type = error_type` → `error.type = error_type` (tracing field syntax: `"error.type" = error_type`). Remove `kind = $kind` and `run_id = $run_id` from the **exported attribute set** only if plan 005 has not landed; otherwise coordinate — the safe move now is to keep them flowing to the JSONL serialization path but stop registering them as OTLP-indexed attributes. If the current macro cannot separate the two sinks, keep `kind`/`run_id` for now and leave a `TODO(plan-005)` referencing `plans/codebase-health/005-jsonl-versioned-adapter.md`; the hard requirement in this plan is the `jackin.stage` and `error.type` renames.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → update `observability/otlp/tests.rs` assertions (`error_type == "E014"` becomes `error.type`; `stage` becomes `jackin.stage`). `grep -rn '"error_type"\|error_type =' crates/jackin-diagnostics/src --include='*.rs' | grep -v tests` → no emit-path matches remain (adapter/serde field names for legacy JSONL reading may remain if plan 005 owns them).

### Step 5: Fix the typed facade

In `operation.rs`:
1. `operation_log` and `operation_error` must attach every supplied `(key, value)` attr to the emitted tracing event after validating via `registry::validate`. tracing's macro needs static field names, so attach dynamic attrs via the OTLP span/event API where available and mirror the bounded set into JSONL; if the tracing macro shape blocks per-event dynamic fields, stamp them on the current span (`operation_set_i64_attr` pattern) and record that decision in the module `//!` contract.
2. `operation_error` takes an `event_name: &'static str` parameter (registered, dotted) instead of hardcoding `"error"`; `error_type` becomes the `error.type` field; category comes from the registry def.
3. Fix the `Warn` arm: outcome must be a registered outcome supplied by the caller or defaulted per def — never hardcoded `success` on a warning.
4. Add `OperationGuard::complete(outcome: Outcome, error_type: Option<&'static str>)` which records `event.outcome`, `error.type`, and sets span status `ERROR` on `Failure`; `Drop` without explicit completion records `cancelled` (per contract, completion fields are declared; do not let `?`-exits silently record success). Update the one production caller cluster in `crates/jackin-docker/src/shell_runner.rs` (`:98,259,394`) to call `complete` — this is the sole facade consumer outside jackin-diagnostics, confirm with `grep -rn "enter_operation\|operation_error" crates --include='*.rs' | grep -v jackin-diagnostics`.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-docker --all-features` → pass; `operation/tests.rs` gains cases: attrs survive to exporter, warn-level outcome not `success`, guard drop-without-complete records `cancelled`, `operation_error` exports `error.type` and a registered event name.

### Step 6: Sweep spellings in run.rs and finish

`grep -n "expected_shutdown" crates/jackin-diagnostics/src -r` — replace canonical-output occurrences with `expected_close` (a legacy-adapter mapping may keep the old token only inside plan 005's adapter; if run.rs writes it straight into JSONL today, rename and update fixtures). Update `crates/jackin-diagnostics/README.md` structure table with the registry module row. Run the full gates.

**Verify**: `cargo fmt --check` → exit 0; `cargo clippy -p jackin-diagnostics --all-targets --all-features -- -D warnings` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

- New: `registry/tests.rs` fail-closed suite (step 2 list).
- Updated: `observability/otlp/tests.rs` — canonical keys (`error.type`, `jackin.stage`, `expected_close`, dotted registered names); model each updated test on the existing `InMemoryLogExporter` pattern already in that file.
- Updated: `operation/tests.rs` — attr preservation, completion outcomes, warn outcome.
- Verification: `cargo nextest run -p jackin-diagnostics --all-features` → all pass including new tests.

## Done criteria

- [ ] `grep -rn "kind.replace" crates/jackin-diagnostics/src` → no matches
- [ ] `grep -rn "expected_shutdown" crates/jackin-diagnostics/src --include='*.rs' | grep -v adapter` → no matches outside a clearly named legacy adapter
- [ ] `grep -rn 'let _ = attrs' crates/jackin-diagnostics/src/operation.rs` → no matches
- [ ] Registry `validate` rejects `error_type`, `log.category`, unknown keys (tests prove it)
- [ ] `cargo nextest run -p jackin-diagnostics --all-features` exits 0
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/codebase-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The excerpts above don't match the live code (drift).
- The `tracing` macro static-field constraint makes per-event dynamic attributes impossible AND span-stamping is also unavailable — report the constraint instead of stringifying everything into the body.
- Changing `outcome_for`/`event_taxonomy` breaks more than ~15 fixture assertions outside jackin-diagnostics (signals a hidden consumer of the old names; enumerate them and stop).
- You find yourself wanting to edit `crates/jackin-usage` beyond reading — that's plan 004.

## Maintenance notes

- Every future event goes through `EventDef` registration; reviewers should reject PRs adding inline `tracing::info!(target: JSONL_TARGET, …)` with ad-hoc keys.
- Plan 003 replaces the `event.name` attribute with the true top-level `EventName`; keep the attribute mirror until then.
- Plan 009's conformance matrix asserts the registry-validated shape end to end; expect its assertions to be the long-term guard for this plan's invariants.
