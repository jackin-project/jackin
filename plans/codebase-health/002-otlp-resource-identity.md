# Plan 002: Move run/session/component identity off the OTLP Resource onto records and spans

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row in `plans/codebase-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-diagnostics/src/observability/`
> On drift, compare "Current state" excerpts against live code; mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/codebase-health/001-telemetry-event-registry.md
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

The OTLP wire contract in the codebase-health roadmap requires the Resource to carry stable source identity only (`service.name`, version, runtime/platform, SDK identity) and explicitly prohibits `parallax.run.id`, `session.id`, and `jackin.component` there: "Store `parallax.run.id`, `session.id`, and `jackin.component` on each applicable log and span, never in OTLP Resource. … Two runs of the same build must share resource identity while differing in record attributes." Today all three are stamped into the Resource, so every run mints a new Resource identity, backends cannot group by build, and the conformance matrix item "Every capture asserts that Resource excludes run/session/component identity" can never pass. Existing exporter-backed tests lock the wrong shape in and must be flipped.

## Current state

- `crates/jackin-diagnostics/src/observability.rs:778-786` — host Resource:

```rust
fn build_resource(run_id: &str) -> Resource {
    let attributes = vec![
        KeyValue::new(keys::SERVICE_NAME, "jackin"),
        KeyValue::new(keys::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
        KeyValue::new(keys::COMPONENT, "host"),
        KeyValue::new(keys::RUN_ID, run_id.to_owned()),
    ];
    Resource::builder().with_attributes(attributes).build()
}
```

- `crates/jackin-diagnostics/src/observability.rs:891-902` — `capsule_resource(session_id, run_id)` additionally stamps `SESSION_ID` and conditionally `RUN_ID` into the Resource.
- Record-side today: the JSONL/OTLP macro emits a bare `run_id` field (`observability.rs:1628`), not `parallax.run.id`; `operation_span` (`operation.rs:33-49`) stamps neither run id nor component; `session.id` is only in the capsule Resource plus `record_capsule_activity` span attrs.
- Tests asserting the wrong shape: `observability/otlp/tests.rs:499-508` (`wire_log_resource_carries_run_id_service_and_component`) and `:149-166`; keep their names honest when flipping (rename to `…_resource_excludes_run_and_component`).
- Key consts live in `observability.rs` `otel_keys` (module starts line 26): `RUN_ID` = `parallax.run.id`, `SESSION_ID`, `COMPONENT` — confirm exact const names by reading `otel_keys` before editing.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Crate tests | `cargo nextest run -p jackin-diagnostics --all-features` | all pass |
| Lint | `cargo clippy -p jackin-diagnostics --all-targets --all-features -- -D warnings` | exit 0 |
| Cross-crate | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-diagnostics/src/observability.rs`, `observability/otlp/tests.rs`, `observability/tests.rs`, `src/run.rs` (record-side stamping), `src/screen.rs` (span-side stamping), their sibling `tests.rs` files.

**Out of scope**: capsule bridge macros (`jackin-usage`) — plan 004 stamps the capsule-side record attrs; `backend_query_hint`/UI surfaces; the JSONL file schema (plan 005); metric dimensions (contract forbids run/session ids as metric dims — do not add them there).

## Git workflow

Branch `refactor/otlp-resource-identity`; Conventional Commits; `git commit -s`; push after every commit.

## Steps

### Step 1: Shrink the Resources

`build_resource` keeps only `SERVICE_NAME` + `SERVICE_VERSION` (add runtime/platform/SDK identity only if already available without new deps). `capsule_resource` likewise; delete its `session_id`/`run_id` params and update callers (`init_capsule_tracing` path).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → only Resource-shape tests fail (expected; fixed in step 3).

### Step 2: Stamp identity on records and spans

- Records: in the emit path (post plan 001, the registry-validated emit), add `parallax.run.id` (value from the active run — `run.rs` already threads `run_id` into the macro) and `jackin.component` per record; capsule-side `session.id` stamping happens in plan 004, but host-side records that already know a session id must carry it.
- Spans: in `operation_span` (`operation.rs`) and the screen/launch span creation (`screen.rs`), stamp `parallax.run.id` (from `crate::run::active_run()` accessor — check `run.rs` re-exports; `active_run` is exported from `lib.rs:48`) and `jackin.component`. Use the existing `set_attribute` pattern under `#[cfg(feature = "otlp")]`.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features -E 'test(/otlp/)'` → record/span assertions can read `parallax.run.id` as an attribute.

### Step 3: Flip the Resource tests

Rewrite `wire_log_resource_carries_run_id_service_and_component` and the `:149-166` capture to assert: Resource contains `service.name`/`service.version`; Resource does **not** contain `parallax.run.id`, `session.id`, or `jackin.component`; those three appear on record/span attributes where applicable. Add one test proving two sequential inits with different run ids produce equal Resources.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → all pass. `cargo xtask ci --fast` → exit 0.

## Test plan

Flipped Resource tests + the two-runs-share-Resource test + span-attr presence tests, modeled on the existing `InMemoryLogExporter`/`InMemorySpanExporter` patterns in `observability/otlp/tests.rs`.

## Done criteria

- [ ] `grep -n "COMPONENT\|RUN_ID\|SESSION_ID" crates/jackin-diagnostics/src/observability.rs` shows none of the three inside `build_resource`/`capsule_resource`
- [ ] Exporter-backed test asserts Resource exclusion (all three keys)
- [ ] `parallax.run.id` + `jackin.component` present on exported records and operation/screen spans in tests
- [ ] `cargo nextest run -p jackin-diagnostics --all-features` exits 0; `cargo xtask ci --fast` exits 0
- [ ] Status row updated

## STOP conditions

- Excerpts don't match live code.
- You cannot find a non-Resource path to make `session.id` available to capsule records without editing `jackin-usage` — record that dependency and stop (plan 004 territory).
- Any downstream code (grep `backend_query_hint`, `configured_endpoint`) turns out to parse Resource attributes for run correlation — enumerate and stop.

## Maintenance notes

- Plan 009's matrix re-asserts Resource exclusion continuously.
- If a future collector config groups by Resource, it now correctly groups by build, not by run — document that in the observability reference page when plan 009 lands docs updates.
