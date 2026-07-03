# Plan 004: Route `record_direct` events through tracing so container crashes, build steps, and timings reach OTLP — and clean the exported attribute set

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics/src/run.rs crates/jackin-diagnostics/src/observability.rs`
> Plans 001–003 legitimately touched these files (test seam, filter builder,
> severity plumb). Compare the excerpts below against live code; proceed if the
> `record_direct` structure is intact, STOP otherwise.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/001-otlp-export-test-seam.md (tests), plans/003-error-severity-truth.md (severity plumb it reuses)
- **Category**: bug
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

`RunDiagnostics` has two write paths. Events routed through `emit_jsonl_event` become `tracing` events and reach both the JSONL file layer and the OTLP bridge. Events routed through `record_direct` write **only** to the JSONL file — and in the primary production configuration (OTLP active, `JACKIN_DIAGNOSTICS_FILE` unset) the file writer is `None`, so those events are **written nowhere**: only an aggregate count survives into the run summary. The events on the file-only path are exactly the ones an operator needs in the backend: `container_started`, `container_exited`/`container_crash` (with exit code, OOM flag, and crash-log evidence), `docker_build_step`, `timing_started`/`timing_done`, and the run-start marker. A container crash under OTLP mode currently produces no log record, no span, no error — nothing but a counter.

Separately, the events that DO export carry noise the backend surfaces as junk facets: literal `stage="<none>"` / `detail="<none>"` sentinels, a `jackin_jsonl=true` implementation marker, and a `diagnostics_message` attribute that byte-duplicates the body. This plan routes everything through tracing and cleans the exported field set.

## Current state

`crates/jackin-diagnostics/src/run.rs:612-644` — the bypass:

```rust
fn record_direct(&self, kind: &str, message: &str, stage: Option<&str>,
                 detail: Option<&str>, span_id: Option<&str>) {
    self.record_metrics(kind);
    // Counts above always update ...; the JSONL write only happens when the file sink is on.
    let Some(writer) = &self.writer else { return; };
    let event = JsonEvent { ts_ms: now_ms(), run_id: &self.run_id, trace_id: &self.run_id,
                            span_id, kind, message, stage, detail };
    ...
}
```

Callers of `record_direct` (all lose OTLP): run-start `run.rs:192-198`; `timing_started` `:385`; `timing_done` `:410`; `container_started` `:496`; `container_exited`/`container_crash`/`container_crash_log` `:541-549`; `docker_build_step` `:567`; `record_from_layer` `:576-585` (the JSONL layer's own callback — see the loop hazard below); `record_otlp_internal` `:601` (must STAY file-only — re-entrancy, see below).

The tracing path — `observability.rs:1155-1203` (`emit_jsonl_event_with_level`): emits on `target: JSONL_TARGET` with fields `jackin_jsonl=true`, `run_id`, `kind`, `diagnostics_message=message`, `stage=stage.unwrap_or("<none>")`, `detail=detail.unwrap_or("<none>")`, format message `"{message}"`. The JSONL layer (`observability.rs:69-124`) picks these up via `DiagnosticsEventVisitor`, drops `<none>` sentinels (`:159-160`), resolves the run (explicit `run_id` field → registry → active run, `:110-114`), and calls `run.record_from_layer(...)` → `record_direct` → file write. **The OTLP bridge exports the same event with the raw field set** — sentinels, marker, duplicate and all (pinned by plan 001 tests 1 and 4).

Re-entrancy invariants (do not break):

- `record_otlp_internal` (`run.rs:596-610`) runs INSIDE the diagnostics layer while handling an `opentelemetry*` event — emitting a tracing event there would re-enter the subscriber. It must keep calling `record_direct` directly. Same for the layer callback path `record_from_layer`.
- The comment at `run.rs:604-606` documents this.

Crash evidence content note: `container_exited`'s `crash_evidence` is a 40-line `docker logs`/`multiplexer.log` tail assembled in `crates/jackin-runtime/src/runtime/launch/exit_diagnosis.rs:87-129` — under `--debug` those lines can be capsule byte dumps. Plan 005 owns redaction/caps; THIS plan must land the routing with a **hard size cap** as an interim guard (see Step 3) so it does not widen the export of raw payloads unboundedly before 005 lands.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format / lint | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests | `cargo nextest run --all-features` ; `cargo nextest run -p jackin-diagnostics --all-features` | all pass |

## Scope

**In scope**:

- `crates/jackin-diagnostics/src/run.rs`
- `crates/jackin-diagnostics/src/observability.rs`
- `crates/jackin-diagnostics/src/tests.rs`, `crates/jackin-diagnostics/src/observability/otlp/tests.rs`
- `docs/content/docs/reference/runtime/diagnostics.mdx` (JSONL-contract note if the file shape changes — it must NOT; see Step 2)

**Out of scope**:

- Redaction content rules (plan 005) beyond the interim size cap.
- New span shapes (plan 007). `event.name`/taxonomy renames (plan 006).
- The capsule bridge (`jackin-usage`), `emit_debug_line`, and every call-site crate.

## Git workflow

- Propose branch `fix/diagnostics-otlp-routing`; wait for operator confirm. Conventional commits with `-s`, push after each.

## Steps

### Step 1: Split emit from file-write

In `run.rs`, refactor so the *only* callers of `record_direct` are (a) the JSONL layer callback (`record_from_layer`) and (b) `record_otlp_internal` — the two re-entrancy-constrained paths. Every other current caller switches to the tracing path:

- `timing_started`/`timing_done` → `emit_jsonl_event(&self.run_id, "timing_started"/"timing_done", msg, Some(stage), Some(&event_detail))`.
- `container_started` → `emit_jsonl_event(..., "container_started", ..., Some(container_name), Some(&detail))`.
- `container_exited` → route via the severity plumb from plan 003: `container_crash` kind → `emit_jsonl_error`; clean `container_exited` → `emit_jsonl_event`.
- `container_crash_log` → `emit_jsonl_error` with the capped evidence as `detail` (Step 3).
- `docker_build_step` → `emit_jsonl_event`.
- run-start marker (`run.rs:192-198`) — CAREFUL: it fires during `RunDiagnostics::start`, potentially before `activate()` puts the run into the global slot. The JSONL layer resolves the run by the explicit `run_id` field first (`observability.rs:110-114`) and `start()` registers into `run_registry()` before this point (`run.rs:180-183`) — verify that ordering holds after your edit (registry insert must precede the first emit), then switch it to `emit_jsonl_event` too.

The JSONL file shape must remain byte-identical for these events (same `kind`, `message`, `stage`, `detail` strings) because the layer reconstructs `JsonEvent` from the same fields — the render-conformance fixture extractor (`cargo xtask pty-fixture`) and `summarize_run_file` consume this file. `summary.rs` parses `kind`/`stage`/`detail`; run `cargo nextest run -p jackin-diagnostics --all-features` and the summary tests will catch drift.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → all existing JSONL-shape tests still pass (they assert file content; the file content path is unchanged: tracing event → layer → `record_from_layer` → `record_direct` → same `JsonEvent`).

### Step 2: Confirm span-id parity

The old direct path passed `span_id: None` (or an explicit id); the layer path derives `span_id` from the current tracing scope (`observability.rs:104-108`). After Step 1, timing/container events emitted inside a stage span pick up that span's id — an improvement, but assert the JSONL for a timing event inside `stage()`'s span still matches what `tests.rs:226-255` (stage span-id sharing test) expects. If any existing test asserted `span_id` absent for timing events, update it with a comment noting the deliberate improvement.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → pass.

### Step 3: Interim evidence cap

In `container_exited`, before emitting `container_crash_log`, cap the evidence: keep the LAST 4096 bytes (`char_boundary`-safe slice) and prefix with `"(truncated to last 4096 bytes)\n"` when cut. Constant `const CRASH_EVIDENCE_EXPORT_CAP: usize = 4096;` with a comment naming plan 005 as the owner of real redaction. The full uncapped tail still reaches the operator via the error body built in `exit_diagnosis.rs` (out of scope here) — this cap only bounds the telemetry record.

**Verify**: new unit test `crash_evidence_is_capped` (feed 10 KiB, assert emitted detail ≤ 4096 + prefix).

### Step 4: Clean the exported attribute set

In `emit_jsonl_event_with_level` (`observability.rs:1155-1203`):

1. Drop the `<none>` sentinels from the *emitted* event: tracing requires a fixed field set per macro callsite, so keep the fields but emit `tracing::field::Empty` for absent `stage`/`detail` — the OTel appender skips `Empty` fields, and the JSONL visitor already treats missing as `None`. Concretely: use `stage = tracing::field::Empty` via `Span`-style field recording is not available on events — **events cannot record Empty then fill**. Alternative that works for events: keep two macro arms (with-stage / without-stage) — a 2×2 explosion with detail. Given the constraint, the pragmatic approach: keep passing `"<none>"` into the event BUT filter it in BOTH consumers — the JSONL visitor already does (`:159-160`); for OTLP, the bridge exports raw fields, so instead **stop relying on the bridge's raw mapping**: emit the attributes under the final names with sentinel replaced by empty string `""`, and accept `""` attributes? NO — empty-string facets are the same junk.
   **Decision (do this)**: split the emit into four concrete match arms over `(stage.is_some(), detail.is_some())`, each with its own `tracing::event!` field list (no sentinel anywhere). Four arms × 3 severities is too much duplication for `macro`-free code — use a local helper macro inside the function file:

```rust
macro_rules! jsonl_event {
    ($lvl:expr, $($fields:tt)*) => {
        tracing::event!(target: JSONL_TARGET, $lvl, jackin_jsonl = true, $($fields)*)
    };
}
```

   and 4 arms per level via `tracing::Level` as a const in `event!`. `tracing::event!` accepts a runtime-ish level only via `Level` const — it accepts `Level` expressions in `event!(level, ...)` form since tracing 0.1.38 (`event!(level: lvl, ...)` — check the workspace tracing version in Cargo.lock; if the pinned version rejects expression levels, keep the existing three-way `if/else` over levels and put the 4-arm match inside each, generated by the local macro).
2. Remove the `diagnostics_message` duplicate field: the JSONL visitor maps `"diagnostics_message" | "message"` (`observability.rs:158`) — the format message already arrives to the visitor via... NO: the visitor only sees *fields*, and the format message IS field `message` in tracing's model. Confirm: `record_debug`/`record_str` receive the `message` field for the format string — the visitor's `"message"` arm exists precisely for it. So dropping `diagnostics_message` keeps the JSONL intact via the `message` field. Do it, run the crate tests; if JSONL `message` goes missing, the visitor's assumption was wrong — restore and STOP.
3. Keep `jackin_jsonl = true` as the layer's routing marker? The layer already filters by `metadata.target() != JSONL_TARGET` (`observability.rs:71`) — the bool is belt-and-braces (`:95-97`). Remove the field from the event AND the `jackin_jsonl` check from the visitor (target check is sufficient and single-sourced); this removes the exported `jackin_jsonl` facet.

Update plan 001's pinned tests: `sentinel_none_values_are_exported` → inverted (`stage` attribute ABSENT when None); `exported_log_carries_body_and_attributes` → no `diagnostics_message`, no `jackin_jsonl`.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → all pass with updated assertions.

### Step 5: Full-workspace gate + docs

`reference/runtime/diagnostics.mdx`: if it enumerates exported attributes, update the list (remove `jackin_jsonl`/`diagnostics_message`, note absent-vs-`<none>` change). The JSONL FILE format is unchanged — state that explicitly in the PR body (the file is the versioned-ish contract per that page).

**Verify**: `cargo fmt --check`; clippy `-D warnings`; `cargo nextest run --all-features` → exit 0.

## Test plan

- Updated plan-001 characterization tests (Step 4).
- New: `container_crash_reaches_otlp_when_file_off` — using the plan 001 seam, construct a `RunDiagnostics` with `writer = None` (add `#[cfg(test)] fn for_test_no_writer(run_id)` constructor if `start()` cannot be coerced; keep it minimal) OR simpler: any `RunDiagnostics` under the test subscriber — call `container_exited("jk-x", 137, true, "/log", Some("evidence"))` → assert one ERROR log with `kind=container_crash` attrs + one `container_crash_log` record, both in `logs.get_emitted_logs()`.
- `timing_events_reach_otlp`: `timing_started/done` → two exported records.
- `crash_evidence_is_capped` (Step 3).
- File-shape regression: existing `tests.rs:97,113,127,146,171,206,226` must stay green untouched (they pin the JSONL file contract).
- Pattern: `crates/jackin-diagnostics/src/observability/otlp/tests.rs` from plan 001.

## Done criteria

- [ ] `rg -n "record_direct" crates/jackin-diagnostics/src/run.rs` → callers are exactly `record_from_layer` + `record_otlp_internal` (plus the fn def)
- [ ] Plan 001 seam test proves `container_crash`, `docker_build_step`, `timing_done` produce exported OTLP records
- [ ] Exported records carry no `jackin_jsonl`, no `diagnostics_message`, no `<none>` attributes (updated seam tests green)
- [ ] JSONL file shape unchanged: all pre-existing `tests.rs` file-content tests pass unmodified (except any `span_id`-absence assertion updated per Step 2)
- [ ] `cargo nextest run --all-features` / clippy / fmt exit 0
- [ ] `plans/README.md` row updated

## STOP conditions

- Emitting the run-start marker via tracing before `activate()` drops the event (registry lookup fails) — report; the fix ordering (`run_registry` insert before emit) is load-bearing.
- The pinned tracing version rejects both expression-level `event!` and the local-macro fallback without gross duplication.
- Removing `diagnostics_message` loses the JSONL `message` field (visitor assumption false).
- Any `xtask pty-fixture` / summary test fails on file shape — the file contract broke; revert the offending step and report.
- You need to touch `exit_diagnosis.rs` or any call-site crate to make routing work.

## Maintenance notes

- After this plan, `record_direct` is the *sink*, not an API — a lint-style guard (comment on the fn: "callers: layer + otlp_internal only; emit via emit_jsonl_event") is in place; reviewers should reject new direct callers.
- Plan 005 replaces the 4096-byte interim cap with real redaction + artifact routing; plan 006 renames attribute keys (`kind` → `event.name` etc.) and owns backend-facing naming.
- The run summary's `event_counts` semantics: counts now increment in `record_from_layer` (post-layer) for routed events — verify `emit_run_summary` totals in existing tests still match (they count via `record_metrics` inside `record_direct`, which the layer path still hits — no change expected; flag if a test disagrees).
