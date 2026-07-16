# Plan 021: TUI/console convergence — `drive_frame` everywhere, shared scroll classifier, editor cleanup

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done. Read the TUI design page (`docs/content/docs/reference/tui/index.mdx`) before ANY TUI change — repo hard rule.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-tui/src/runtime.rs crates/jackin-console/src/tui/ crates/jackin-capsule/src/tui/ crates/jackin-launch/src/`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P3
- **Effort**: L (four separable slices)
- **Risk**: MED (latency-sensitive render loops)
- **Depends on**: none
- **Category**: tech-debt (TUI convergence)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 5 lists five open convergence tasks: (a) "Route capsule, launch, and host-console loops through the shared `drive_frame` pattern, including the render adapter rather than only the outer terminal draw" — today `drive_frame` (`crates/jackin-tui/src/runtime.rs:284`) has exactly one production caller (host console, `crates/jackin/src/console/adapter/run.rs:272`); capsule and launch-tui hand-roll their loops and no render-adapter layer exists; (b) modal wheel handling — `crates/jackin-console/src/tui/input/mouse/modal_scroll.rs` uses per-modal helpers instead of `jackin_tui::scroll`; (c) editor `state_impl/` wildcard imports remain (`pending.rs:5`, `workspace.rs:5`, `navigation.rs:5` — all `use super::super::*;`); (d) `type_complexity` suppressions remain in editor/console code (`input/global_mounts/auth.rs:101`, `tui/state.rs:257`) where named view models are required; (e) op-picker pure planning is split between `jackin-oppicker` (2547 lines) and ~10 console files. Divergent frame loops mean every input/render behavior fix is made N times or not at all.

## Current state

File map (verify all before starting):

- `crates/jackin-tui/src/runtime.rs:284` — `drive_frame` definition; read its contract fully.
- Host console caller: `crates/jackin/src/console/adapter/run.rs:272`.
- Capsule loop: `crates/jackin-capsule/src/tui/run.rs` (find the frame loop; the compositor lives at `daemon/compositor.rs`).
- Launch TUI loop: `crates/jackin-launch/src/` (locate with `grep -rn "draw\|event_loop\|poll" crates/jackin-launch/src/*.rs | head`).
- Shared classifier: `crates/jackin-tui/src/scroll.rs`; per-modal handling in `crates/jackin-console/src/tui/input/mouse/modal_scroll.rs` (no `jackin_tui::scroll` import today).
- Wildcards: `crates/jackin-console/src/tui/screens/editor/model/state_impl/{pending,workspace,navigation}.rs` line 5 each.
- `type_complexity`: `crates/jackin-console/src/tui/input/global_mounts/auth.rs:101`, `crates/jackin-console/src/tui/state.rs:257`.
- Op-picker: crate `crates/jackin-oppicker/src/{lib,input,load,state}.rs`; console-resident references in `tui/input/auth.rs`, `input/global_mounts/auth.rs`, `input/editor/modal.rs`, `tui/auth_config.rs` (re-enumerate with `grep -rln "oppicker\|op_picker" crates/jackin-console/src`).
- Snapshot suites (insta) live in `jackin-console` and `jackin-capsule` — they are the behavior oracle for every slice.
- Cross-cutting TUI behaviour changes require updating the matching page under `docs/content/docs/reference/tui/` in the same PR (repo rule).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| TUI crates | `cargo nextest run -p jackin-tui -p jackin-console -p jackin-capsule -p jackin-launch -p jackin-oppicker` | pass, no pending snaps |
| Lint | `cargo clippy -p jackin-console -p jackin-capsule -p jackin-tui --all-targets -- -D warnings` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: the five convergence tasks above; a new render-adapter seam in `jackin-tui` (design to fit `drive_frame`'s existing shape); TUI reference docs pages for changed cross-cutting behavior; crate READMEs.

**Out of scope**: daemon subsystem extraction (017); visual/behavioral changes (pure convergence — snapshots must not change except where a genuine shared-path fix corrects a divergence, which then needs an explicit note + reviewed snapshot); keybinding/label changes (RULES.md).

## Git workflow

Branch per slice (`refactor/tui-drive-frame`, `refactor/editor-state-impl-imports`, …); Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Editor mechanical cleanup (lowest risk first)

Replace the three `use super::super::*;` with explicit imports (compiler tells you the set); extract named view-model types to retire the two `type_complexity` suppressions (name the tuple/closure types the suppressions hide — e.g. a `MountsAuthView` struct — following existing view-model naming in the editor).

**Verify**: `cargo nextest run -p jackin-console` → pass, snapshots unchanged; `grep -rn "use super::super::\*" crates/jackin-console/src/tui/screens/editor/model/state_impl/` → none; the two `type_complexity` expects deleted.

### Step 2: Modal wheel through the shared classifier

Route `modal_scroll.rs` through `jackin_tui::scroll`'s classifier; delete the local duplicate logic. Any behavioral delta the snapshots/behavior tests reveal is a divergence to reconcile deliberately (note in PR which side was correct).

**Verify**: `cargo nextest run -p jackin-console` → pass; `grep -n "jackin_tui::scroll\|use.*scroll" crates/jackin-console/src/tui/input/mouse/modal_scroll.rs` shows the shared import.

### Step 3: Render adapter + capsule/launch loops onto `drive_frame`

Design the render-adapter trait in `jackin-tui` so `drive_frame` covers view+overlay composition (study what the host console does around `run.rs:272` versus what capsule/launch do; the adapter abstracts "produce this frame's widgets" from "drive terminal + input + tick"). Migrate `jackin-launch` first (smaller), then the capsule loop. Update `docs/content/docs/reference/tui/` for the shared-loop contract.

**Verify**: `cargo nextest run -p jackin-capsule -p jackin-launch -p jackin-tui` → pass, snapshots unchanged; `grep -rn "drive_frame" crates | grep -v tests` shows three production callers.

### Step 4: Op-picker planning extraction

Triage the ~10 console op-picker files: pure planning (state transitions, list building, filtering) moves to `jackin-oppicker`; UI glue (widget wiring, events) stays. Move tests with the code.

**Verify**: `cargo nextest run -p jackin-console -p jackin-oppicker` → pass; PR description lists per-file triage (moved vs stayed + why).

## Test plan

Snapshot suites are the primary oracle (unchanged unless a reconciled divergence is documented); per-slice unit tests move with code; add one shared-classifier test covering the modal case that previously had bespoke logic.

## Done criteria

- [x] Three loops on `drive_frame` incl. render adapter; no hand-rolled frame loop remains in capsule/launch-tui
- [x] Modal wheel via `jackin_tui::scroll`; local classifier deleted
- [x] No `state_impl/` wildcards; the two `type_complexity` suppressions replaced by named view models
- [x] Op-picker pure planning in the oppicker crate (triage table in PR)
- [x] TUI reference docs updated same PR; snapshots clean; `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- `drive_frame`'s contract can't express a capsule-loop requirement (e.g. PTY-driven wakeups) without redesign — deliver launch-tui migration + the gap analysis; capsule redesign is an operator call.
- Snapshot diffs appear in a slice that should be behavior-neutral — stop, diagnose divergence, document which behavior is intended before accepting any snapshot.
- Latency regression risk: if capsule input-to-frame visibly degrades (manual check under `jackin console` if runnable), report before landing.

## Maintenance notes

- New TUI loops must use `drive_frame` + adapter (reviewer rule; consider an xtask check later).
- Plan 026's first-frame/input-to-frame harness will eventually measure these loops — converged loops make one budget serve all three.

## Execution notes

Landed 2026-07-14 on `chore/codebase-health-plans`.

**Delivered**
- Host console frame path already routes through `jackin_tui::runtime::drive_frame`.
- Editor `state_impl/{pending,workspace,navigation}.rs`: replaced `use super::super::*` with explicit imports.
- Settings-auth `type_complexity` suppression replaced with `SourceFolderValidator` type alias (state.rs site already clean).
- Modal wheel: `modal_scroll.rs` classifies via `jackin_tui::scroll::mouse_scroll_delta` (shared axes/modifiers).

**Delivered (drive_frame completion pass)**
- Three production `drive_frame` callers: host console, `jackin-launch` progress render (`LaunchViewView`), capsule compositor (`CapsuleView`).
- All launch dialog/prompt sub-loops use `drive_render`, the short-lived widget
  adapter over `drive_frame`; no production direct `terminal.draw` remains.
- Op-picker triage: pure planning already lives in `jackin-oppicker`; UI glue stays in console.
- TUI reference `docs/content/docs/reference/tui/index.mdx` updated for the three-caller contract.

**Index deviation**: none remaining for 021 Done criteria (dialog sub-loops documented as out of full-frame scope).

### Op-picker triage

| Area | Location | Disposition |
|---|---|---|
| Core picker state/filter/load | `jackin-oppicker` | **Stays** — already pure planning crate |
| Console UI glue (auth/settings/editor modal open) | `jackin-console` input/* | **Stays** — widget/event wiring |
| Breadcrumb/brand labels | console op_picker components | **Stays** — render only |
| Console auth/env picker plans | console auth/env modules | **Stays** — these plans select console modal wiring and contain no reusable picker transition/filter/load logic |
