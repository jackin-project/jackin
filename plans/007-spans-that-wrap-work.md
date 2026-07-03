# Plan 007: Make spans wrap real work — stage guards with readable names, subprocess duration/outcome, coverage for cleanup and git pull

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics/src/run.rs crates/jackin-docker/src/shell_runner.rs crates/jackin-runtime/src/runtime/launch/git_pull.rs crates/jackin-runtime/src/runtime/cleanup.rs crates/jackin-launch-tui/src/progress.rs`
> Earlier plans touched run.rs and shell_runner.rs; re-verify excerpts, STOP on
> structural contradiction.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/001-otlp-export-test-seam.md, plans/002-export-filter-allowlist.md, plans/003-error-severity-truth.md
- **Category**: bug / perf
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

The OTLP trace surface is nearly empty of meaning:

- Host business code contains **zero** spans (`#[instrument]` count: 0; manual span count outside jackin-diagnostics: 1, in jackin-usage). The only launch spans are `launch_stage` spans that (a) all share one constant name — a live trace showed 12 indistinguishable `launch_stage` waterfall rows with the meaningful stage only in the attributes panel — and (b) are entered only around each event *emission* (`run.rs:339-347`), never held across the stage's work, so **span durations are ~0 and the waterfall timing is fiction** (a trace-detail page showed a 123 s trace whose rows explain nothing).
- The subprocess choke point `ShellRunner` — through which every `docker`/`git`/external command runs — records no duration and no exit code on success; slow launches cannot be attributed to a command from telemetry.
- Cleanup (763-line `cleanup.rs` + the Ctrl+C `load_cleanup.rs` path) and `git pull` success are telemetry-silent — precisely the failure-prone paths that run when things are going wrong.

`tracing-opentelemetry` (0.33) honors `otel.name` for dynamic span naming and `otel.status_code`/`otel.status_description` for status — the mechanics this plan uses are already proven in this repo (`observability.rs:937`, `screen.rs:117`).

## Current state

Stage span creation — `crates/jackin-diagnostics/src/run.rs:296-347` (verified): `stage()` on `stage_started` inserts `tracing::info_span!("launch_stage", stage = stage)` into `stage_spans`; per event it does `let _entered = span.enter();` around one `emit_jsonl_event`; on `stage_done|failed|skipped` it removes the span (drop closes it). Durations are wall-clocked separately via `stage_starts` and written into `detail` JSON. `stage_span_for` at `:422-437`.

Stage kinds arrive from `crates/jackin-launch-tui/src/progress.rs:109-163` (verified): `emit_stage` → `diagnostics.stage(kind, stage.label(), &detail, None)` with kinds `stage_started|stage_progress|stage_done|stage_skipped|stage_failed`. `LaunchStage::label()` values are the stage names (e.g. "derived image").

ShellRunner — `crates/jackin-docker/src/shell_runner.rs` (verified head): `log_command` (`:63-76`) debug-only command text; `run`/`do_capture` check `status.success()` but no `Instant`, no success event, no exit-code event.

git pull — `crates/jackin-runtime/src/runtime/launch/git_pull.rs:117-140` (verified): `record_git_pull_results` emits `active_debug` on success (debug tier only) and `run.compact("git_pull", "git pull failed in {src}")` on failure; no duration, no success compact; separate `tracing::warn!` + `eprintln!` in `print_git_pull_results` (`:89-104`).

Cleanup — `crates/jackin-runtime/src/runtime/cleanup.rs`: 2 telemetry-ish hits total (verified via grep); `load_cleanup.rs`: zero.

Timings — `active_timing_started/done` (`run.rs:379-420`): JSONL events + summary histograms; no span events.

Screen/trace infra to reuse: `screen.rs::launch_trace` wraps the launch future in the `launch` screen span (`screen.rs:210-235`) — per-stage spans created inside it nest correctly IF they are real children (they are: `info_span!` inherits the current span as parent when entered inside `launch_trace`'s instrumented future).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests | `cargo nextest run --all-features` | pass |
| Span inventory | `rg -n "info_span!|#\[instrument" crates/ --type rust` | before/after comparison |

## Scope

**In scope**:

- `crates/jackin-diagnostics/src/run.rs` — stage-span lifetime + naming (`otel.name`), status on failure, timing span events
- `crates/jackin-docker/src/shell_runner.rs` — duration + exit-code + outcome emission
- `crates/jackin-runtime/src/runtime/launch/git_pull.rs` — success/duration events, de-duplicate the triple emit
- `crates/jackin-runtime/src/runtime/cleanup.rs`, `crates/jackin-runtime/src/runtime/launch/load_cleanup.rs` — timing + outcome coverage
- Tests: plan-001 seam + affected crates' `tests.rs`
- `docs/content/docs/reference/runtime/diagnostics.mdx` — trace-shape section

**Out of scope**:

- New spans for HTTP/download internals beyond the completion events listed (full `http.client.request` spans are a follow-up — Maintenance).
- `--telemetry-level` (008), metrics (012), the capsule daemon's internal loops.
- Changing `LaunchStage` enum or the cockpit UI.

## Git workflow

- Propose branch `feat/telemetry-spans-wrap-work`; operator confirm; `git commit -s` per step; push each.

## Steps

### Step 1: Stage spans get real lifetimes and readable names

In `run.rs::stage()`:

1. On `stage_started`, create the span with a normalized dynamic name via `otel.name`:

```rust
let otel_name = format!("launch.{}", normalize_stage_name(stage)); // "derived image" -> "derived_image"
tracing::info_span!("launch_stage", stage = stage, otel.name = %otel_name)
```

Add `fn normalize_stage_name(stage: &str) -> String` (lowercase, spaces/`-`→`_`) with unit tests. Keep the tracing-name `launch_stage` constant (tracing requires a const name; `otel.name` overrides the exported name — the mechanism `screen.rs:117` already uses).
2. Span lifetime: the span must COVER the stage. The map already keeps the span object alive from `stage_started` to `stage_done` (`stage_spans`); the problem is only that it is entered per-emission. For the *exported* duration, `tracing-opentelemetry` measures from span creation to close (busy/idle split) — creation happens at `stage_started` and close at removal on `stage_done` (drop). **Verify this premise with the seam first** (see Test plan test 1: exported span end−start ≈ the wall time between stage_started and stage_done, not ~0). If the premise holds (expected — OTel spans time create→close, not enter→exit), the existing keep-alive map already yields correct exported durations once nothing else is broken; the fix reduces to naming + status + parenting. If exported duration is ~0, STOP and report (the layer would be timing entered scopes — plan needs redesign around explicit `span.end()`).
3. Parenting: `stage_started` may be called from a different task than `launch_trace`'s instrumented future. Check one call path: `progress.rs::emit_stage` runs inside the launch flow (same task) — `info_span!` inherits correctly. Leave as-is.
4. Status: on `stage_failed`, before dropping the span, record status:

```rust
span.record("otel.status_code", "ERROR");
span.record("otel.status_description", message);
```

`record` on non-declared fields is a no-op — declare them at creation with `tracing::field::Empty`:
`info_span!("launch_stage", stage = stage, otel.name = %otel_name, otel.status_code = tracing::field::Empty, otel.status_description = tracing::field::Empty)`.

**Verify**: seam tests 1–3 below pass.

### Step 2: Subprocess duration + outcome in ShellRunner

In each exec arm (`run`, `do_capture` — locate all `status()`/`output()` awaits; there are four arms per the audit):

- `let started = std::time::Instant::now();` before spawn; after wait, one always-on event through the existing diagnostics channel:

```rust
if let Some(run) = jackin_diagnostics::active_run() {
    run.compact("subprocess_done", &format!("{program} exited"));
}
```

is NOT enough structure — instead call a new small helper `jackin_diagnostics::subprocess_done(program, elapsed_ms, exit_code)` added beside `active_timing_done` in `run.rs`, which emits `emit_jsonl_event(run_id, "subprocess_done", "subprocess exited", Some(program), Some(&json!({"elapsed_ms":…,"exit_code":…}).to_string()))` — body stable, facts in detail (plan 006 rule). Program name only (never argv — argv is the redaction surface).
- Emit for success AND failure (failure keeps the existing bail flow; the event is additive before the bail).
- Frequency guard: `subprocess_done` is per external command — lifecycle-frequency, fine for the always-on tier.

**Verify**: seam test `subprocess_done_carries_duration_and_exit`; `cargo nextest run -p jackin-docker --all-features` green.

### Step 3: git pull + cleanup coverage

1. `git_pull.rs`: wrap the pull execution site with `active_timing_started/done("repo", "git_pull", None)` (find the actual spawn — `record_git_pull_results` is post-hoc; grep `fn run_git_pulls|git_pull` in the file for the execution fn). On success add `run.compact("git_pull", "git pull succeeded")`; the failure arm keeps ONE emission path: keep `run.compact` + `active_debug(stderr)` and DELETE the duplicate `tracing::warn!` lines (`:95,99,103` — they are invisible in the no-OTLP default build and double-report under OTLP; the compact event is now the single durable record). Keep the operator-facing `eprintln!` warnings (unchanged UX).
2. `cleanup.rs`: around each purge unit (containers/images/networks/volumes/roles/index — the file's section functions), add `active_timing_started/done("cleanup", "<resource>", None)` and per-resource outcome `run.compact("cleanup", …)` on failure only (success stays quiet; counts ride the timing detail). `load_cleanup.rs` (cancel path): add `timing_started/done("cleanup", "cancel_cleanup", …)` around its body + a compact on entry (`"cancel cleanup started"`) — this is the path that runs when the operator Ctrl+C's a wedged launch; it must leave a trace.
3. Timings as span events: in `timing_done` (`run.rs:394-420`), if a current span is active, also `tracing::Span::current().record(...)`? — span *events* are what map to OTel span events: emit `tracing::info!(parent: &span, duration_ms, name = %key, "timing")`… Simpler and sufficient: the plan-004 routing already makes `timing_done` a tracing event; when it fires inside a stage span (Step 1 keeps spans alive), the OTel log record carries the span context, and `tracing-opentelemetry` ALSO records events inside spans as span events when they pass the span layer's filter. Confirm via seam: a `timing_done` fired between `stage_started`/`stage_done` appears with the stage span's context (test 4). No extra code if it holds.

**Verify**: crate tests + seam test 4.

### Step 4: Docs + gate

`reference/runtime/diagnostics.mdx`: trace-shape section — exported stage spans are named `launch.<stage>`, carry real durations, `ERROR` status on failure; subprocess/cleanup/git-pull event coverage.

**Verify**: fmt / clippy / `cargo nextest run --all-features` → exit 0.

## Test plan

Seam (plan 001 infra, `observability/otlp/tests.rs`):
1. `stage_span_duration_covers_stage`: `stage("stage_started","derived image",…)`, sleep 50 ms, `stage("stage_done",…)` → exported span duration ≥ 50 ms (proves the Step-1 premise).
2. `stage_span_exported_name_is_stage_specific`: exported name == `launch.derived_image` (updates plan-001's `launch_stage_span_name_is_constant` pin).
3. `failed_stage_span_has_error_status`: `stage_failed` → span status Error with description.
4. `timing_event_inherits_stage_span_context`: `timing_done` between start/done carries the stage span's span_id/trace_id on the exported log record.
5. `subprocess_done_carries_duration_and_exit` (via the new helper directly).
Unit: `normalize_stage_name` ("derived image"→`derived_image`, "Sidecar"→`sidecar`).
Pattern: plan 001 tests.

## Done criteria

- [ ] Seam tests 1–5 green; plan-001 constant-name pin updated
- [ ] `rg -n "tracing::warn!" crates/jackin-runtime/src/runtime/launch/git_pull.rs` → no matches
- [ ] `rg -n "active_timing|compact\(\"cleanup" crates/jackin-runtime/src/runtime/cleanup.rs crates/jackin-runtime/src/runtime/launch/load_cleanup.rs` → coverage present in both files
- [ ] `subprocess_done` emitted from every ShellRunner exec arm (grep the file: 4 call sites or a shared tail fn)
- [ ] fmt/clippy/nextest green; `plans/README.md` updated

## STOP conditions

- Seam test 1 shows ~0 duration (premise false — exported spans time enter/exit, not create/close). STOP; the fix then needs explicit OTel span end control, a different design.
- `otel.status_code` recorded via `span.record` doesn't reach the exported status (check `tracing-opentelemetry` 0.33 handles post-creation `record` for status fields — if only creation-time fields work, set status by recording BEFORE close via declared-Empty fields as written; if that still fails, STOP).
- Cleanup instrumentation requires making `cleanup.rs` functions async-aware of the run handle in a way that cascades signature changes beyond the two cleanup files.

## Maintenance notes

- Follow-up (deferred): full `http.client.request`-style spans in `net.rs`/`agent_binary.rs` with status/bytes (events land in plan 012's metric work; spans when a consumer needs waterfall placement). Also `#[instrument]` on `usage:refresh` provider probes is already present in jackin-usage — leave.
- Reviewer scrutiny: span explosion risk — only stage-level and subprocess-level units here; reject per-loop-iteration spans.
- Parallax plan 012 (their waterfall redesign) renders `operation + stage`; exported names from this plan are what it will show.
