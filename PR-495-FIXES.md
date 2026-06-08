# PR #495 — Goal Index

- **PR:** https://github.com/jackin-project/jackin/pull/495 (`refactor: finish TUI architecture epic`, DRAFT → `main`)
- **Branch:** `feature/tui-architecture`
- **Evidence audit:** folded into these goal files and the *Already landed* table below. The original `PR-495-REVIEW.md` (a point-in-time static analysis that never ran `cargo build`/`clippy`/tests) was removed once its findings were confirmed done or tracked here; git history retains it.
- **Ground truth re-verified at HEAD:** `f920b29a` (2026-06-08). The status in every phase file below was checked against this commit, not inherited from the audit.

This file is the **entry point and dashboard**. It does not hold task detail. Each runnable goal lives in its own file under [`pr-495-goals/`](pr-495-goals/), and that file is the single authoritative home for its tasks' status. Run one phase at a time.

> These `PR-495-*.md` files are disposable operator working docs at the repo root. They are outside the `docs/**` and `.github/**` globs the docs/spell CI scans (`.github/workflows/docs.yml:189-192`), so they do not affect PR checks. Delete them after merge.

## How to run this as a goal

Two modes, same files.

### Mode A — one goal for the whole PR

Point the runner at this index. It is self-describing — roster, rules, helper map, and a per-phase status ledger — so a single goal can drive every phase:

```
/goal Follow PR-495-FIXES.md
```

The runner must:

1. Read this index first: invariants, shared-helper map, source-of-truth refs, and the **Already landed** table (skip everything in it).
2. Walk the **Goal roster** top to bottom (phase 0 → 7). Open each phase file and execute its task table.
3. Keep exactly one task `in_progress`. On finishing a task: update its status row **in that phase file**, run the row's verify command, then commit + push.
4. **The per-phase status tables are the durable ledger.** After a context reset/compaction, re-read the phase files and resume at the first non-`done` task — do not restart completed work. Status lives in the file, not in memory.
5. Stop at a phase boundary if blocked and record the reason in the row; do not silently skip ahead.
6. At the end, produce the **Completion report** (bottom of this file).

> Caveat: this is a large change (8 phase files). One uninterrupted goal run will exceed a single context. That is fine — because every task commits + pushes and the status tables persist, the run is resumable: re-invoke the same command and it picks up where the ledger says. If your `/goal` runner does not auto-resume, run Mode B per phase.

### Mode B — one phase at a time

Point the runner at a single phase file for tighter context and review control:

```
/goal Follow pr-495-goals/20-debug-info.md
```

### Binding rules (both modes)

Binding rules for every run:

1. **Verify before acting.** Each task row carries a status and a HEAD `f920b29a` evidence pointer. Before touching a `pending` task, re-run its evidence check (the `file:line` or command in the row). If it now reads as already handled, mark it `done` with the new evidence and move on — do **not** redo landed work. If it diverges from the row, update the row.
2. **One active phase.** Keep exactly one task `in_progress`. Finish or explicitly `defer` it before starting a non-dependent task.
3. **`done` means verified.** Code changed, tests/snapshots/docs updated where the row says, and the row's verify command passed (or its failure is documented with cause).
4. **`deferred` means tracked.** Only when a roadmap item or follow-up names the exact remaining files/behaviors. "Too large" is not enough.
5. **No local styling forks.** A fix that adds a second colour, border, hint, row, scroll, or click style is wrong — extend or extract the shared helper named in the row instead.
6. **Docs land with code.** If a rule changes, update the matching `docs/content/docs/reference/tui/*.mdx` page in the same change. The published docs are the spec.

## Goal roster

Run in this order unless a task's evidence proves a different dependency. Rollup is a summary; the phase file is authoritative.

| Order | Goal file | Scope | Task IDs | Rollup |
|---|---|---|---|---|
| 0 | [`pr-495-goals/00-preflight.md`](pr-495-goals/00-preflight.md) | Orient, confirm landed work, settle spec gaps | `PRE-1`–`PRE-3`, `ARCH-0` | landed/verify |
| 1 | [`pr-495-goals/10-architecture.md`](pr-495-goals/10-architecture.md) | Lint adoption, enum-count reconcile, finish console/launch extraction | `ARCH-1`–`ARCH-3` | 2 real, 1 done |
| 2 | [`pr-495-goals/20-debug-info.md`](pr-495-goals/20-debug-info.md) | Launch copy wiring, footer-hint placement, hover smoke | `DBG-1`–`DBG-3` | 2 real, 1 smoke |
| 3 | [`pr-495-goals/30-scroll-hints.md`](pr-495-goals/30-scroll-hints.md) | Overflow-derived scroll hints everywhere | `SCR-1`–`SCR-3` | 2 real, 1 audit |
| 4 | [`pr-495-goals/40-capsule-panes.md`](pr-495-goals/40-capsule-panes.md) | Pane chrome palette, vertical scrollback, thumb reuse | `CAP-1`–`CAP-3` | 2 real, 1 partial |
| 5 | [`pr-495-goals/50-dialogs-rows.md`](pr-495-goals/50-dialogs-rows.md) | Git prompt, gutters, action rows, error dialog | `DLG-1`–`DLG-5` | 5 real |
| 6 | [`pr-495-goals/60-roadmap-reconcile.md`](pr-495-goals/60-roadmap-reconcile.md) | Re-status prematurely-closed roadmap items; deferred enhancements | `RMP-1`–`RMP-6` | reconcile/deferred |
| 7 | [`pr-495-goals/90-verify.md`](pr-495-goals/90-verify.md) | The actual merge blockers + closeout | `CI-1`–`CI-4` | 3 real, 1 keep-green |

## ✅ Already landed at HEAD `f920b29a` (do not redo)

The audit listed these as the highest-priority work. They are **done** on this branch — re-verified live. A goal run that "fixes" them is chasing ghosts.

| Was | Status now | Evidence |
|---|---|---|
| Delete ~32k LOC / 67 orphaned `runtime/` + `isolation/` files | done | `crates/jackin/src/runtime/` absent; `isolation/` holds only `tests.rs` (the file the audit said to keep). |
| Remove `jackin-diagnostics → jackin-tui` dependency | done | `crates/jackin-diagnostics/Cargo.toml` deps only `jackin-core`. |
| `cargo fmt` failing | done | `cargo fmt` check passes (`gh pr checks 495`). |
| DCO failing | done | DCO passes. |
| Container-info label/test mismatch | done | `nextest` passes; canonical label `"jackin version"` (`crates/jackin-tui/src/components/container_info.rs:129`). |
| Launch Debug-info "jackin version" shows the JSONL path | done | Distinct `run_log_path` + `jackin_version` params threaded (`crates/jackin-launch/src/tui/view.rs:25-29,131`; `…/components/container_info_dialog.rs:58`). |
| Launch Debug-info backdrop transparent over digital-rain | done | `Clear` widget clears area first (`crates/jackin-launch/src/tui/view.rs:32`). |
| Launch Debug-info horizontal-scroll clip leak | done | Clipping validated by `container_info` tests (`crates/jackin-tui/src/components/container_info/tests.rs:278-315`). |
| Capsule Debug-info scroll not persisted / bespoke | done | `DialogBodyScroll` persisted + threaded; shared `dialog_scroll_axes` / `render_scrollable_dialog_body` used end-to-end (`crates/jackin-capsule/src/tui/components/dialog.rs:198,384,451,672`). |
| Capsule pane scrollbar shows when content fits | done | Gated on `filled > 0`; `tail_vertical_thumb` returns `None` at no overflow (`crates/jackin-capsule/src/tui/view.rs:351`; `crates/jackin-tui/src/scroll.rs:444`). |
| Launch build-log + Debug-info footer hints not axis-derived | done | Both derive from `ScrollAxes` (`…/components/build_log_dialog.rs:16`, `…/container_info_dialog.rs:64`). |
| Extracted config tests stranded in the consumer crate | done | Config tests live in `crates/jackin-config/src/**` (`editor/tests.rs`, `app_config/tests.rs`, `resolve/tests.rs`, …); none remain under `crates/jackin/src/config/`. |
| `jackin-term` `present_frame` bench missing | done | `crates/jackin-term/benches/present_frame.rs` + `[[bench]]` in its `Cargo.toml` (with `dhat-heap`). |
| Dispatch-arm doc claimed "~17" vs reality | done | Roadmap corrected to "58 production arms across 6 files" (`post-restructure-fixes-checklist.mdx`, `agent-runtime-trait.mdx`). |
| CI clippy gate differed from the documented gate | done | `ci.yml:220` runs `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`. |
| 6 bare `unreachable!()` in `console/tui/state.rs` | done | None remain; file decomposed 1789→926 LOC. |
| Shared primitives built but surfaces not migrated | done (largely) | `HoverTracker` / `FocusOwner` / `modal_lifecycle` adopted across capsule, launch, console; residual root-console usage folds into `ARCH-3`. |

## Non-negotiable TUI invariants

Every task is judged against these (full text in the source-of-truth refs):

- **One meaning, one shape.** Same concept on two screens uses the same component/primitive. No per-surface forks of Debug info, error dialogs, scrollable panels, hint bars, selectable rows, copyable values.
- **Reusable first.** Search `crates/jackin-tui`, the surface's `src/tui`, and root-console adapters before adding TUI code. A pattern appearing in >1 surface moves to `jackin-tui` before merge.
- **Ratatui is the path.** Compose via `Frame`/`Buffer`/`Rect`/`Layout` + shared components. Raw ANSI is limited to documented tail/backend duties.
- **Elm boundaries.** Model owns state, `update` is deterministic, `view` is pure, external work travels through typed effects. Components never run Docker/git/fs/network.
- **Footer-only, true-affordance hints.** Hints live in the fixed bottom row. A scroll hint appears only when that axis's scrollbar is visible and can move; a copy hint only when a copy target is real.
- **Focus is visible and singular.** Exactly one container per layer holds the bright `PHOSPHOR_GREEN` cue. No competing green border / visible `▸` on parents behind a child dialog.
- **Stable gutter.** Hiding `▸` never moves row text; the two-cell cursor gutter is always reserved.
- **One scroll geometry.** Renderer, input, hit-testing, scrollbars, hover/copy overlays, and hints derive from the same content extents, viewport rect, and clamped offsets — via `jackin_tui::scroll`, `scrollable_panel`, `dialog_layout`, `DialogBodyScroll`, `ScrollAxes`.
- **Default background, named colours.** Surfaces use terminal-default tokens, never forced black or inline RGB.
- **Idiomatic Rust 2024.** Self-named modules, `lints.workspace`, typed enums over stringly dispatch, no `unwrap`/`expect` on runtime input, no broad `allow`.

## Shared-helper ownership (extend these; do not re-implement)

| Concern | Owner |
|---|---|
| Offset clamp, thumb metrics, cursor-follow, drag/track geometry | `jackin_tui::scroll` |
| Bordered passive scroll blocks, panel focus border, scrollbars | `jackin_tui::components::scrollable_panel` |
| Scrollable dialog bodies, per-axis overflow, scroll hints | `jackin_tui::components::dialog_layout` (`DialogBodyScroll`, `dialog_scroll_axes`, `scroll_hint_spans`, `render_scrollable_dialog_body`) |
| Debug-info rows, copy/hyperlink geometry | `jackin_tui::components::container_info` |
| Selectable rows, `▸` gutter, full-width highlight | `jackin_tui::components::select_list` / shared row helper |
| Panel chrome, body inset, title padding, focus border | `jackin_tui::components::panel` |

## Source-of-truth references (acceptance criteria — read before editing)

| Reference | Governs |
|---|---|
| `docs/content/docs/reference/tui/components.mdx` | Reuse hard rule, component homes, settings/workspace parity |
| `docs/content/docs/reference/tui/architecture.mdx` | Elm boundaries, source locations, typed effects, render purity |
| `docs/content/docs/reference/tui/dialogs.mdx` | Modal sizing, five-slot padding, Debug-info contract, footer-only hints, modal click lifecycle |
| `docs/content/docs/reference/tui/chrome.mdx` | Bottom-chrome order, status/chip behavior, focus borders |
| `docs/content/docs/reference/tui/navigation.mdx` | W3C keyboard roles, hover/click affordances, cursor gutter, scroll hint/scrollbar coupling |
| `docs/content/docs/reference/tui/visual-design.mdx` | PHOSPHOR palette, default-bg fills, action-row style, copyable value styling |
| `docs/content/docs/reference/adrs/adr-003-ratatui.mdx` | Ratatui as the accepted render library |
| `docs/content/docs/reference/adrs/adr-004-pane-body-rendering.mdx` | `PaneBodyWidget` custom-body boundary |
| `crates/AGENTS.md` · `crates/jackin-tui/COMPONENTS.md` | Module layout, lint inheritance; component inventory |
| `docs/content/docs/reference/tui/lookbook/*.mdx` + `docs/public/tui-lookbook/*.svg` | Visual regression references |

## Global verification gates

Cargo (green at HEAD — keep them green):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features
```

Docs (these contain the **live failing checks** — see `pr-495-goals/90-verify.md`):

```sh
cd docs && bun run build && bun run check:repo-links && bunx tsc --noEmit && bun test
```

PR:

```sh
gh pr checks 495
```

## Completion report (produce at end of the final phase)

- Phases/tasks completed, with each task's final status.
- Tasks still `pending`/`deferred`, with exact reason and remaining files/behaviors.
- Shared components/helpers changed or added.
- Docs and lookbook artifacts updated.
- Verification commands run and their result.
- Residual risk — especially capsule vertical scrollback (`CAP-2`) and any roadmap deferral.
