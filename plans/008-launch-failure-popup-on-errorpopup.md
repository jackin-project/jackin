# Plan 008: Port the launch failure popup onto the shared ErrorPopup

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-launch-tui/src/tui/components/failure_dialog.rs crates/jackin-launch-tui/src/tui/view.rs crates/jackin-launch-tui/src/tui/subscriptions.rs crates/jackin-tui/src/components/error_dialog.rs`
> On mismatch with "Current state": STOP. (Plan 007 landing IS expected drift in error_dialog.rs — verify its structured-rows API exists, then proceed.)

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/007-tui-error-dialog-canonical.md (hard dependency — provides `ErrorPopupRow` + `row_value_rects`)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

The launch cockpit owns a parallel 483-line red-border error modal (`failure_dialog.rs`) — its own border block, sizing, label/value rows, OK affordance — while the docs designate the shared `ErrorPopup` as the only error surface. Error-surface fixes (scroll, copy, styling) do not propagate to the launch failure popup, and its genuinely valuable features (OSC-8 hyperlink rows, per-row copy targets, reveal actions) are locked inside one surface. After plan 007, the shared ErrorPopup can express structured rows; this plan ports the failure popup's *shell and rows* onto it, keeping only launch-specific row construction and click/copy semantics local.

## Current state

`crates/jackin-launch-tui/src/tui/components/failure_dialog.rs` (public surface, verified):

```
:16  pub struct FailurePopupRow { label: &'static str, value: String,
                                  copy_target: Option<FailureCopyTarget>, href: Option<String> }
:24  pub fn failure_popup_rows(failure: &LaunchFailure, run_id: &str) -> Vec<FailurePopupRow>
:88  pub fn failure_popup_rect_for_rows(area, rows) -> Rect        // sizing
:161 pub fn failure_popup_value_rect(...)                          // per-row value geometry
:217 pub fn failure_copy_target_at(...)                            // click → copy target
:246 pub fn failure_popup_block_rect(...)
:262 pub fn failure_popup_body_metrics(...)
:277 pub fn failure_copy_payload(...)  :293 pub fn failure_reveal_payload(...)
:371 pub fn render_failure_popup(frame, area, view, failure, run_id, debug_mode)
        // builds its own red Block::default().borders(ALL), centered "  OK  ",
        // ModalBackdrop via launch_overlay_chrome_areas
:444 pub fn failure_popup_hyperlink_overlay(...)                   // OSC-8 byte overlay
```

Callers (all in-crate):
- `crates/jackin-launch-tui/src/tui/view.rs:14,85` — `render_failure_popup(...)`; `:134,:162-176` — `failure_popup_hyperlink_overlay_bytes` wraps `failure_popup_hyperlink_overlay`.
- `crates/jackin-launch-tui/src/tui/subscriptions.rs:22,351-358,419` — click dispatch via `failure_copy_target_at` + `failure_copy_payload`; hover at `:419`.

Post-007 shared API (verify it exists before starting): `ErrorPopupState { title, message, rows: Vec<ErrorPopupRow> }` with `ErrorPopupRow { label, value, href }`, `render_error_dialog_in`, and `row_value_rects(inner) -> Vec<Rect>`.

Design canon: `dialogs.mdx` §"Error Surface — ErrorPopup Only"; §"Long values in dialogs" (OSC-8 hyperlink is the preferred approach; "its `R` key reveal action must use the same row values instead of re-deriving paths" — that sentence is *about this popup*).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-launch-tui -p jackin-tui` then `cargo nextest run` | pass |
| Runtime smoke (only if operator asks) | n/a — launch TUI needs Docker; rely on the crate's PTY fixtures | — |

## Scope

**In scope**:
- `crates/jackin-launch-tui/src/tui/components/failure_dialog.rs` (shrinks: shell/row-render/geometry deleted; row construction + copy/reveal payloads + hyperlink overlay stay)
- `crates/jackin-launch-tui/src/tui/view.rs`, `subscriptions.rs` (call-site rewiring)
- `crates/jackin-tui/src/components/error_dialog.rs` ONLY if the rows API needs a parameter the port reveals (per plan 007's maintenance note: extend there, never fork here)

**Out of scope**:
- `LaunchFailure` construction and diagnostics-path logic.
- The launch chrome (`launch_overlay_chrome_areas`) and rain/progress components.
- Capsule error surface (plan 009).

## Git workflow

Branch (operator confirm): `refactor/launch-failure-on-errorpopup`. `git commit -s` + push per commit. Update `dialogs.mdx` "debug info dialog contract"/failure-popup references if they name `render_failure_popup` (grep the docs; adjust names in same PR).

## Steps

### Step 1: Map rows

Convert `FailurePopupRow` construction (`failure_popup_rows`, `:24-72`: run id / run diagnostics / docker output / next) to build `Vec<jackin_tui::ErrorPopupRow>` for display, and keep a parallel launch-local `Vec<Option<FailureCopyTarget>>` (index-aligned) for click semantics. Keep `failure_copy_payload`/`failure_reveal_payload` untouched — they must read from the same row values (the docs' "same row values" rule).

**Verify**: `cargo nextest run -p jackin-launch-tui` → existing row-construction tests pass (update types only).

### Step 2: Render through the shared popup

Rewrite `render_failure_popup` to: keep `launch_overlay_chrome_areas` + `ModalBackdrop` (launch chrome is legitimate), build `ErrorPopupState` (title = failure title, message = failure summary, rows from Step 1), and call `render_error_dialog_in(frame, rect, &state)`. Sizing: replace `failure_popup_rect_for_rows` internals with the shared popup's height estimation if compatible; if the shared estimate differs from the launch popup's wrapped-row math, extend the shared estimator (plan 007 file) rather than keeping a local fork — smallest change wins, but the *border/button/row painting* must come from the shared component.

**Verify**: `cargo nextest run -p jackin-launch-tui` → pass; `rg 'Block::default' crates/jackin-launch-tui/src/tui/components/failure_dialog.rs` → 0 matches.

### Step 3: Re-anchor hit-testing and hyperlinks on shared geometry

Replace the geometry internals of `failure_popup_value_rect`/`failure_copy_target_at`/`failure_popup_block_rect`/`failure_popup_body_metrics` with lookups into `row_value_rects` from the shared state (thin launch-local wrappers mapping rect-index → `FailureCopyTarget` may remain; duplicate rect *math* may not). `failure_popup_hyperlink_overlay` keeps emitting OSC-8 bytes but sources its rects from the same shared geometry.

**Verify**: existing click/hover tests in `subscriptions.rs` tests (or wherever `failure_copy_target_at` is tested — `rg 'failure_copy_target_at' crates/jackin-launch-tui --glob '*test*'`) pass unchanged or with mechanical rect updates; `cargo nextest run` → pass.

### Step 4: Shrink audit

`failure_dialog.rs` should now contain: row construction, copy/reveal payloads, copy-target mapping, hyperlink byte overlay, launch-specific wrappers — and zero border/button/row painting. Confirm the file shrank materially.

**Verify**: `rg -c 'fn ' crates/jackin-launch-tui/src/tui/components/failure_dialog.rs` — record before/after in the PR body; full sweep fmt/clippy/nextest → exit 0.

## Test plan

- Keep every existing failure-popup test green (they define current behavior).
- Add: a buffer-render test asserting the ported popup shows all four row labels (`run id`, `run diagnostics`, `docker output`, `next`) and the DANGER_RED border — model on existing render tests in the crate (`rg 'Buffer' crates/jackin-launch-tui/src --glob '*test*'` for the pattern).
- Add: rect-alignment test — `failure_copy_target_at` returns the right target when clicking inside each `row_value_rects` rect (this is the regression the old duplicate-geometry code risked).

## Done criteria

- [x] fmt / clippy / `cargo nextest run` exit 0
- [x] `rg 'Block::default|"  OK  "' crates/jackin-launch-tui/src/tui/components/failure_dialog.rs` → 0
- [x] Popup renders via `render_error_dialog_in` (grep confirms the call)
- [x] Click/copy/reveal tests pass with geometry sourced from `row_value_rects`
- [x] `plans/README.md` updated

## STOP conditions

- Plan 007's rows API cannot express a needed behavior (per-row copy badge glyph, wrapped multi-line values) — STOP and report the exact gap; the fix belongs in plan 007's file, and may need operator sign-off on the shared API shape.
- PTY/e2e fixtures assert exact failure-popup bytes that shift by more than styling (`cargo nextest run -p jackin --features e2e --profile docker-e2e` failures) — report before touching fixtures.
- The hyperlink overlay byte format depends on hand-rolled geometry in a way `row_value_rects` cannot reproduce cell-exactly.

## Maintenance notes

- Future error-dialog features (scrollable body, copy-all) now land once in `error_dialog.rs` and the launch popup inherits them.
- Reviewer: check no rect math is duplicated between `failure_dialog.rs` and `error_dialog.rs`; check reveal (`R`) and click use the same row values.
- Deferred: migrating the launch build-log dialog's ANSI word-wrap (separate concern, see plans/README.md backlog note).
