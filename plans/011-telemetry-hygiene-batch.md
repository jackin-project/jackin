# Plan 011: Telemetry-infrastructure hygiene batch — lock-poison consistency, flush cadence, mouse coalescing, log rotation, bounded maps

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics/src/run.rs crates/jackin-usage/src/logging.rs crates/jackin-launch-tui/src/tui/subscriptions.rs`
> Plans 003–007 reshape run.rs; locate each item's code by symbol, not line,
> and STOP only if a symbol is gone.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none strictly; if plans 003/004 are landed, apply the items to the reshaped code (symbols are stable)
- **Category**: bug / perf
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

Six small, independent defects in the telemetry substrate itself — each cheap alone, together they make the pipeline degrade under exactly the conditions it exists for (panics, floods, long sessions):

1. **Inconsistent lock-poison handling** silently disables stage/timing/metrics recording for the rest of a run after any panic while a telemetry mutex is held.
2. **Per-event `flush()`** turns the JSONL `BufWriter` into a syscall-per-line under a global lock — the concrete cost of the `--debug` firehose.
3. **Per-pixel mouse telemetry** (`cockpit-dialog-mouse`) writes+flushes one event per raw mouse move with a dialog open — 21,611 rows observed in one live store.
4. **`multiplexer.log` grows unbounded** — append-only, no rotation; a `--debug` soak fills the host-mounted state dir.
5. **First-notice TOCTOU** in `record_otlp_internal` can double-print the "telemetry export issue" operator notice.
6. **Unbounded per-run maps** (`stage_spans`, `timing_starts`, histograms) leak entries for unmatched starts across a long `jackin console` session.

## Current state

(All verified firsthand at `5d3661cff` unless noted.)

1. Poison: recovering pattern `.lock().unwrap_or_else(std::sync::PoisonError::into_inner)` at `run.rs:88,182,205,426,641,679,687`; silent-skip pattern `if let Ok(mut …) = self.<field>.lock()` at `run.rs:298,301,311,318,321,381,402,647` (fields: `stage_starts`, `stage_spans`, `timing_starts`, `stage_durations_ms` via `durs`, `metrics`).
2. Flush: `record_direct` (`run.rs:639-643`): `drop(writeln!(guard, "{line}")); drop(guard.flush());` per event.
3. Mouse: `emit_dialog_mouse_debug_telemetry` (`crates/jackin-launch-tui/src/tui/subscriptions.rs:476-490`, verified) fires for every `Event::Mouse` incl. `Moved` when `is_debug_mode()` and a dialog is open.
4. Rotation: `crates/jackin-usage/src/logging.rs:55-99` opens `/jackin/state/multiplexer.log` append-only; only a start marker delimits runs; no size check anywhere in the file.
5. TOCTOU: `record_otlp_internal` (`run.rs:596-610`): `first` computed from `metrics.event_counts` under one lock, notice emitted after release; the count increments later in `record_direct`→`record_metrics`.
6. Maps: `RunDiagnostics` fields (`run.rs:68-76`); `stage_spans` removed only on `stage_done|failed|skipped` (`:427-430`), `timing_starts` only on `timing_done` (`:400`); histograms grow per entry forever.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt/clippy | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests | `cargo nextest run --all-features` | pass |

## Scope

**In scope**: `crates/jackin-diagnostics/src/run.rs`, `crates/jackin-usage/src/logging.rs`, `crates/jackin-launch-tui/src/tui/subscriptions.rs`, matching `tests.rs` files, `docs/content/docs/reference/runtime/diagnostics.mdx` (rotation note only).

**Out of scope**: batch/queue sizing of the OTLP exporters (measure after plan 002 removes the storm before tuning); the capsule per-line `LOG_FILE` mutex redesign (bounded-channel writer task — deferred, see Maintenance); severity/shape of any event (003/004/006).

## Git workflow

- Propose branch `fix/telemetry-hygiene`; operator confirm; one `git commit -s` per numbered item (six small commits), push each.

## Steps

### Step 1: Poison consistency

Replace every `if let Ok(mut x) = self.<field>.lock()` in `run.rs` (sites listed above) with the recovering accessor used elsewhere in the same file. Add a tiny private helper to cut repetition:

```rust
fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
```

**Verify**: `rg -n "if let Ok\(mut" crates/jackin-diagnostics/src/run.rs` → no matches; crate tests pass.

### Step 2: Flush cadence

In `record_direct`: keep the per-event `writeln!`, drop the per-event `flush()`. Flush instead: (a) in `ActiveRunGuard::drop` (before `shutdown_otlp()`, `run.rs:84-97`) — add a `pub(crate) fn flush_writer(&self)`; (b) after error-tier events (`kind == "error"` path / `emit_jsonl_error`-originated records — post-plan-003, flush when the routed kind ends with `_failed` or level is error); (c) after `emit_run_summary`. Rationale comment: tail-loss window is bounded by (a); errors force-flush so a crash right after an error keeps the evidence.

**Verify**: existing JSONL tests still pass (they read after the run guard drops — confirm each failing test actually dropped the guard; fix tests that read mid-run by flushing explicitly via the new method). New test: `error_events_flush_immediately` — write error, read file BEFORE guard drop, line present.

### Step 3: Coalesce dialog mouse moves

In `emit_dialog_mouse_debug_telemetry` (`subscriptions.rs:476-490`): emit `Moved` only on hover-state transitions — track last `(col,row)` cell in the caller's state (the fn takes `v: &LaunchView`; add a `last_dialog_mouse_cell: Cell<Option<(u16,u16)>>`-style field to the subscription state that owns the call at `:555-560`) and emit when the CELL changes, not per pixel event; always emit non-`Moved` kinds (Down/Up/Drag/Scroll). Keep the line format identical.

**Verify**: unit test on the extracted decision fn `fn should_emit_dialog_mouse(kind, prev_cell, cell) -> bool` (same-cell Moved → false; cell change → true; Down → true). Grep confirms `kind={:?}` format unchanged.

### Step 4: Rotate multiplexer.log

In `jackin-usage/src/logging.rs::init` (`:55-99`): before opening, if the file exists and exceeds `MAX_LOG_BYTES` (32 MiB), rename it to `multiplexer.log.1` (replacing any existing `.1`) and open fresh — rotation at daemon start only (no mid-run rotation: keeps `write_line` untouched and `tail -f` semantics intact mid-session). Constant + one-line comment.

**Verify**: unit test with `JACKIN_CAPSULE_LOG_PATH` pointing into a tempdir (the documented test seam, `logging.rs:48`): pre-create an oversized file, call `init`, assert `.1` exists and the live file is small. Note: `init` also sets `DEBUG_ENABLED` and the panic hook `OnceLock` — process-global; nextest per-process isolation makes this safe (do NOT run under `cargo test`).

### Step 5: Atomic first-notice

Replace the `first` computation in `record_otlp_internal` with a dedicated `AtomicBool` on `RunDiagnostics` (`otlp_internal_notified: AtomicBool`), `swap(true, Ordering::Relaxed)` deciding the single winner.

**Verify**: unit test — two sequential calls, exactly one notice (assert via `drain_debug_buffer_for_test` after `begin_debug_buffering`, matching the existing notice-buffer test pattern in `tests.rs:471-495`).

### Step 6: Bound the per-run maps

At `emit_run_summary` (end of run): after snapshotting, clear `stage_starts`/`stage_spans`/`timing_starts` and log (compact, `kind="diagnostics"`) any leftover keys as `unclosed: <list>` — a leaked start is itself a diagnostic. Cap histogram vectors: in `record_metrics`/duration pushes, keep at most 1024 entries per key (drop-oldest via `VecDeque` or truncate — simplest: `if v.len() < 1024 { v.push(ms) }` plus a dropped-count; comment the cap).

**Verify**: unit tests — unmatched `timing_started` yields the `unclosed` note in the summary path; 2000 pushes cap at 1024.

## Test plan

Named per step (six tests + adjusted existing ones). Pattern: `crates/jackin-diagnostics/src/tests.rs` for run-file assertions; `run/tests.rs` for pure payload fns.

## Done criteria

- [ ] Steps 1–6 each verified by their named test/grep
- [ ] `cargo nextest run --all-features` / clippy / fmt exit 0
- [ ] diagnostics.mdx notes the 32 MiB rotation
- [ ] Six focused commits, each `-s`-signed and pushed
- [ ] `plans/README.md` updated

## STOP conditions

- Step 2 breaks a test that reads the file mid-run and cannot reasonably flush first — report which consumer depends on per-line durability (that dependency decides the cadence).
- Step 3's state threading requires restructuring the subscriptions event loop beyond adding one field.
- Step 4: the capsule opens the log before `JACKIN_CAPSULE_LOG_PATH` is readable in tests (ordering surprise) — report.
- Any step's change surfaces in operator-visible output other than the intended notice/rotation.

## Maintenance notes

- Deferred deliberately: moving the capsule `write_line` file I/O off the hot path onto a bounded-channel writer task (changes `tail -f` immediacy and needs a flush-on-panic story — measure first after plan 005 removes the payload dumps, which were most of the volume).
- Deferred: OTLP batch-queue sizing + a dropped-records metric (measure post-plan-002; the SDK default may be fine once the storm is gone).
- Reviewer: Step 2's flush points are the crash-evidence guarantee — challenge any further relaxation.
