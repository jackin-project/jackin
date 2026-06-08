# Goal — Phase 2: Shared Debug info

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

Most of the Debug-info audit findings already landed (version wiring, backdrop, h-scroll clip, capsule scroll persistence + shared routing, axis-derived launch footers — see the index "Already landed" table). Three items remain. **Read `docs/content/docs/reference/tui/dialogs.mdx` (Debug-info contract) first** — it is the acceptance spec.

Canonical path for all surfaces: `DebugInfo` → `ContainerInfoState` → `render_container_info`. Screens differ only in which facts they gather.

## Tasks

| ID | Status | Files / evidence | Helper | Verify | Acceptance |
|---|---|---|---|---|---|
| `DBG-1` | pending | `crates/jackin-launch/src/tui/subscriptions.rs:173,370,373` pass `""` for `run_log_path` into the hit-test state builders → `Diagnostics log` row payload is empty, so launch copy/hover targets an empty value | `container_info_copy_payload_at`, `launch_container_info_state` | `cargo nextest run -p jackin-launch` | Launch hover+click on `Run ID` and `Diagnostics log` copy the exact bare run id / JSONL path (non-empty). The hit-test state is built with the same real `run_log_path` used for rendering. |
| `DBG-2` | pending | `crates/jackin-tui/src/components/container_info.rs:385` — `render_debug_info_hint` draws the hint at `dialog_rect.y + height + 1` (a floating row under the dialog). Axis derivation is already correct; **placement** is the bug. | `render_debug_info_hint` + each surface's footer | `cargo nextest run -p jackin-tui -p jackin-launch -p jackin-capsule` | Debug-info key hints render in the surface's fixed footer row (status/separator/hint stack), not as a floating line beneath the dialog box. `dialogs.mdx`/`chrome.mdx` updated to state footer-only (coordinate with `PRE-1`). |
| `DBG-3` | pending (needs smoke) | Capsule hover/click geometry: `crates/jackin-capsule/src/daemon/mouse_input.rs:46`, `…/input_dispatch.rs:600`, `container_info.rs:499`, `dialog_layout.rs:401`. Code paths share the same rect; tests pass; the audit's off-by-one may predate the row-ordering fix. | `container_info_copy_payload_at` | live smoke + `cargo nextest run -p jackin-capsule` | Reproduce in a live capsule Debug-info dialog (hover/click `Run ID`, `Container ID`, `Diagnostics log`). If the copied/highlighted row matches the pointer row, mark `done` with the smoke evidence. If off-by-one reproduces, make every capsule call site pass the identical rendered rect, then add a coordinate test. |

## Detail

### `DBG-1` — launch copy must receive the real path
The render path threads the correct `run_log_path`, but the **mouse hit-test** path rebuilds a `ContainerInfoState` with `""`. Because an empty string is still `Some("")`, the `Diagnostics log` row exists but copies nothing. Pass the same `run_log_path` (and run id) into the hit-test state builders at the three call sites. Then assert, in a launch test, that placing the mouse over each copyable row and clicking yields a non-empty payload equal to the bare run id / JSONL path — and that the version row never contains `.jackin/data/diagnostics`.

### `DBG-2` — hints belong in the fixed footer
`render_debug_info_hint` already computes the right axes; it just paints them one row below the dialog. Move the hint into the surface's reserved footer row so the bottom chrome stays `status → separator → hint`, with no floating line under the box. This is a shared-component change — apply it once in `container_info.rs` (or by having each surface render the hint spans into its footer) so console, launch, and capsule all match. Update the two docs pages to state footer-only and remove any "floating hint" wording. Add a layout test: open Debug info, assert hint text is in the footer row with one separator above the status bar, and no hint text immediately below the dialog rectangle.

### `DBG-3` — verify capsule hover alignment by smoke, not assumption
The static analysis says the geometry is consistent (same `Panel….inner(area)` for render and hit-test). The audit's symptom (hovering `Run ID` copies `Container ID`) was an operator observation that may predate the Run-ID-first ordering fix. Do not refactor on suspicion: run the live capsule dialog with `--debug`, hover and click each row, and read the diagnostics run JSONL. Close as `done` with that evidence, or, if it still mis-targets, unify the rect and add a regression test covering the horizontally-scrolled case.

## Done definition
- `DBG-1`: launch copy test green; payloads exact and non-empty.
- `DBG-2`: hints in footer on all three surfaces; docs updated; layout test green.
- `DBG-3`: closed with live-smoke evidence (fixed or confirmed-correct).
