# PR #495 — Goal

- **PR:** https://github.com/jackin-project/jackin/pull/495 (`refactor: finish TUI architecture epic`, DRAFT → `main`)
- **Branch:** `feature/tui-architecture`
- **Code verified at HEAD `f920b29a`** (doc-only commits since). Every status in the **Master ledger** was checked against the live tree, not inherited from an audit.

This file supersedes the earlier PR #495 fix plan and review-note queue. It is the single working spec for finishing PR #495: one master status ledger, one detail section per phase, one source of truth for status. The ledger is the only place status lives. Disposable operator doc at the repo root (outside the `docs/**` / `.github/**` CI globs); delete after merge.

## Operating contract

1. Read this file once in this order: **Operating contract**, **Master ledger**, **Already landed**, **Non-negotiable TUI invariants**, **Shared-helper ownership**, **Source-of-truth references**, then only the phase sections needed for the current row.
2. Work from **Definition of done**, **Goal checklist**, and the **Master ledger** before choosing implementation order. The checklist gives phase shape; the ledger is the durable task list.
3. Work the **Master ledger** top to bottom. Skip `done` rows and skip **Already landed**. Verify evidence before acting; stale evidence gets corrected in the row.
4. Keep exactly one ledger row `in_progress`. No phase-level status, no second status table, no shadow notes.
5. On finishing a row: update its ledger status, update the matching checklist item when applicable, add any new evidence to the phase detail, run the row's verify command, then commit + push. This makes the ledger resumable across context resets.
6. Status vocabulary: `pending`, `in_progress`, `done`, `deferred`. Nothing else. `done` = code changed + tests/docs/lookbook updated where required + verify passed or failure documented. `deferred` = named roadmap/follow-up with exact remaining files/behaviors.
7. No silent scope shrink. If investigation proves a row is obsolete, mark it `done` with evidence; if it is too large, mark it `deferred` with a concrete follow-up. Do not delete rows to make progress look cleaner.
8. No screenshot-only fixes. Every visual fix needs a code-level cause, a shared-component decision, and a regression test/snapshot where practical.
9. No local styling forks. A fix that adds a second colour, border, hint, row, scroll, or click style is wrong; extend the shared helper named by the row. Docs land with code: if a rule changes, update the matching `docs/content/docs/reference/tui/*.mdx` and roadmap status in the same commit.
10. If blocked by missing live evidence, add durable `cdebug!` telemetry first, record the needed rerun/run id in the row, then stop. End of run -> produce the **Completion report**.

## Master ledger

Verify each row's evidence before acting — if it now reads as already handled, mark it `done` with new evidence and move on.

| ID | Phase | Status | Task | Verify |
|---|---|---|---|---|
| `ARCH-0` | 0 Preflight | done | Orphan trees + `diagnostics→tui` dep already removed; guard against regression | `cargo check -p jackin -p jackin-diagnostics` |
| `PRE-1` | 0 Preflight | done | Reconcile Debug-info backdrop wording across `dialogs.mdx` + `chrome.mdx` | `cd docs && bun run build` |
| `PRE-2` | 0 Preflight | done | Build-log close semantics settled: `Esc`/`q` close; body clicks swallowed; scrollbar clicks stay interactive | `cargo nextest run -p jackin-launch build_log`; docs build |
| `PRE-3` | 0 Preflight | done | Audited Settings vs workspace-editor Auth render paths; `DLG-3` now records the forked renderers and both affected files | read-only |
| `ARCH-1` | 1 Architecture | done | `[workspace.lints]` already adopted in all 17 crates; private lint tables absent; `crates/AGENTS.md` documents inheritance | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`; opt-in count 17 |
| `ARCH-2` | 1 Architecture | done | Dispatch-arm count already corrected to 58 in roadmap docs | docs build |
| `ARCH-3` | 1 Architecture | deferred | Console extraction remains live root-integration work; docs now name exact remaining root modules instead of claiming this PR can finish the move | `cargo build -p jackin-console -p jackin-launch` |
| `DBG-1` | 2 Debug info | done | Launch Debug-info hit-test/copy/scroll state receives real `run_log_path`, not `""` | `cargo nextest run -p jackin-launch`; `cargo clippy -p jackin-launch --all-targets --all-features --locked -- -D warnings` |
| `DBG-2` | 2 Debug info | done | Launch Debug-info hint now renders in the fixed footer row; shared floating helper removed | `cargo nextest run -p jackin-tui -p jackin-launch -p jackin-capsule` |
| `DBG-3` | 2 Debug info | done | Capsule Debug-info hover/click geometry now has rendered-cell coordinate coverage for copy rows | automated coordinate smoke + `cargo nextest run -p jackin-capsule` |
| `SCR-1` | 3 Scroll hints | pending | Capsule main view emits static `↑↓` hint, not overflow-derived | `cargo nextest run -p jackin-capsule` |
| `SCR-2` | 3 Scroll hints | pending | Console workspace footer hint gated on focus bools, not overflow | `cargo nextest run -p jackin-console` |
| `SCR-3` | 3 Scroll hints | pending | Sweep all scroll-hint producers; prove every one is axis-derived | tests |
| `CAP-1` | 4 Capsule panes | pending | Pane border uses gray `FocusPalette::CAPSULE_PANE`, not shared green | `cargo nextest run -p jackin-capsule` |
| `CAP-2` | 4 Capsule panes | pending | Vertical scrollback non-monotonic / flickers (telemetry first) | telemetry + `cargo nextest run -p jackin-capsule` |
| `CAP-3` | 4 Capsule panes | pending | Finish thumb reuse / decide on custom cell-paint | `cargo nextest run -p jackin-capsule` |
| `DLG-1` | 5 Dialogs & rows | pending | Git prompt: five-slot layout + rect/render height parity (8/7 vs 7/6) | `cargo nextest run -p jackin-console` |
| `DLG-2` | 5 Dialogs & rows | pending | File-browser gutter collapses when git child dialog opens | `cargo nextest run -p jackin-console` |
| `DLG-3` | 5 Dialogs & rows | pending | Auth source rows do not reserve a consistent cursor gutter | `cargo nextest run -p jackin-console` |
| `DLG-4` | 5 Dialogs & rows | pending | `+ New workspace` bypasses shared `action_row_style` | `cargo nextest run -p jackin-console` |
| `DLG-5` | 5 Dialogs & rows | pending | `ErrorDialog` double blank row before `OK` (shared component) | `cargo nextest run -p jackin-tui` |
| `RMP-1` | 6 Roadmap reconcile | pending | Diagnostics JSONL not span-sourced; roadmap claims it is | verify + docs build |
| `RMP-2` | 6 Roadmap reconcile | deferred | Observability metrics surface (histograms/counters) not built | docs build |
| `RMP-3` | 6 Roadmap reconcile | deferred | `jackin-term` zero-alloc tail deferred; acceptance over-claimed | docs build |
| `RMP-4` | 6 Roadmap reconcile | deferred | Real PTY conformance corpus absent | docs build |
| `RMP-5` | 6 Roadmap reconcile | pending | Capsule ANSI→Ratatui breadth needs an explicit roadmap item | docs build |
| `RMP-6` | 6 Roadmap reconcile | pending | Stale `[x]` acceptance notes; collapse/justify 2 exception arms | docs build |
| `RMP-7` | 6 Roadmap reconcile | deferred | God-file decomposition (optional, when next touched) | — |
| `CI-1` | 7 Verify | pending | `spell-check-docs` failing on the `docs/` diff | `gh pr checks 495` |
| `CI-2` | 7 Verify | pending | `docs-required` aggregator failing (gates on `CI-1`) | `gh pr checks 495` |
| `CI-3` | 7 Verify | pending | Run docs build/link/type/test gates locally before pushing | `cd docs && bun run build && bun run check:repo-links && bunx tsc --noEmit && bun test` |
| `CI-4` | 7 Verify | done | Cargo gates green at HEAD — keep green after every task | `cargo fmt --check`; clippy; `cargo nextest run --workspace --all-features` |

## Definition of done

PR #495 is not ready until all of these are true:

- Every Debug-info entry point in console, launch, and capsule uses the shared `DebugInfo` -> `ContainerInfoState` -> `render_container_info` path for row order, labels, scroll, copy, hover, links, hints, and clipping.
- Every scrollable dialog/body/panel advertises scroll only for axes that overflow and uses shared scroll state/geometry or a documented lower-level primitive.
- Capsule pane chrome, focus border, scrollbar styling, and footer hints match the shared scrollable panel look represented by Global mounts and the lookbook references.
- Dialogs with actions use canonical five-slot padding in code, tests, and lookbook stories.
- Selectable row text columns do not shift when focus/cursor visibility changes.
- Visible clickable/copy affordances are real: hover style, pointer routing where supported, click-to-action/copy, and hit-geometry tests.
- TUI docs, roadmap status, and lookbook assets are updated in the same row/commit as behavior changes.
- `cargo fmt --check`, clippy, workspace nextest, docs build/link/type/test gates, and PR checks pass or have a row-documented blocker.

## ✅ Already landed at HEAD `f920b29a` (do not redo)

The audit listed these as top-priority. They are done on this branch — re-verified live.

| Was | Evidence |
|---|---|
| Delete ~32k LOC / 67 orphaned `runtime/` + `isolation/` files | `crates/jackin/src/runtime/` absent; `isolation/` holds only `tests.rs` (the keep-file). |
| Remove `jackin-diagnostics → jackin-tui` dependency | `crates/jackin-diagnostics/Cargo.toml` deps only `jackin-core`. |
| `cargo fmt` / DCO failing | Both pass (`gh pr checks 495`). |
| Container-info label/test mismatch | `nextest` passes; canonical label `"jackin version"` (`crates/jackin-tui/src/components/container_info.rs:129`). |
| Launch Debug-info "jackin version" showed the JSONL path | Distinct `run_log_path` + `jackin_version` params threaded (`…/tui/view.rs:25-29,131`; `…/components/container_info_dialog.rs:58`). |
| Launch Debug-info backdrop transparent | `Clear` widget clears area first (`…/tui/view.rs:32`). |
| Launch Debug-info horizontal-scroll clip leak | Validated by tests (`…/components/container_info/tests.rs:278-315`). |
| Capsule Debug-info scroll not persisted / bespoke | `DialogBodyScroll` persisted + shared helpers end-to-end (`…/components/dialog.rs:198,384,451,672`). |
| Capsule pane scrollbar shows when content fits | Gated on `filled > 0`; `tail_vertical_thumb` returns `None` (`…/tui/view.rs:351`; `…/scroll.rs:444`). |
| Launch build-log + Debug-info footer hints not axis-derived | Both derive from `ScrollAxes` (`…/build_log_dialog.rs:16`, `…/container_info_dialog.rs:64`). |
| Extracted config tests stranded in consumer crate | Tests live in `crates/jackin-config/src/**`; none under `crates/jackin/src/config/`. |
| `jackin-term` `present_frame` bench missing | `crates/jackin-term/benches/present_frame.rs` + `[[bench]]` (with `dhat-heap`). |
| Dispatch-arm doc claimed "~17" | Roadmap corrected to "58 arms across 6 files". |
| CI clippy gate differed from documented gate | `ci.yml:220` runs the full `--workspace … --locked` gate. |
| 6 bare `unreachable!()` in `console/tui/state.rs` | None remain; file decomposed 1789→926 LOC. |
| Shared primitives built but not adopted | `HoverTracker` / `FocusOwner` / `modal_lifecycle` adopted across capsule, launch, console. |

## Non-negotiable TUI invariants

Judge every task against these:

- **One meaning, one shape.** Same concept on two screens → same component/primitive. No per-surface forks of Debug info, error dialogs, scrollable panels, hint bars, selectable rows, copyable values.
- **Reusable first.** Search `crates/jackin-tui`, the surface's `src/tui`, and root-console adapters before adding TUI code. A pattern in >1 surface moves to `jackin-tui` before merge.
- **Ratatui is the path.** Compose via `Frame`/`Buffer`/`Rect`/`Layout` + shared components. Raw ANSI limited to documented tail/backend duties.
- **Elm boundaries.** Model owns state, `update` deterministic, `view` pure, external work via typed effects. Components never run Docker/git/fs/network.
- **Footer-only, true-affordance hints.** Hints in the fixed bottom row. A scroll hint appears only when that axis's scrollbar is visible and can move; a copy hint only when a copy target is real.
- **Focus visible and singular.** Exactly one container per layer holds the bright `PHOSPHOR_GREEN` cue. No competing green border / visible `▸` on parents behind a child dialog.
- **Stable gutter.** Hiding `▸` never moves row text; the two-cell cursor gutter is always reserved.
- **Modal backdrop owns body content.** Modal body/background content is cleared to the default background; reserved bottom chrome/status stays visible.
- **Dialog padding is symmetric.** Content-plus-action dialogs use the five-slot inner layout: leading spacer, content, spacer, action row, trailing spacer.
- **One scroll geometry.** Renderer, input, hit-testing, scrollbars, hover/copy overlays, and hints derive from the same content extents, viewport rect, and clamped offsets — via the shared helpers below.
- **Scrollable code is shared code.** Do not reimplement viewport math, thumb math, offset clamping, cursor-follow slicing, wheel routing, or scrollbar hit areas.
- **Clickable targets look clickable.** Resting style is distinct, hover changes color/style, terminals that support it switch to pointer, and hit-test geometry matches rendered target.
- **Default background, named colours.** Surfaces use terminal-default tokens, never forced black or inline RGB.
- **Idiomatic Rust 2024.** Self-named modules, `lints.workspace`, typed enums over stringly dispatch, no `unwrap`/`expect` on runtime input, no broad `allow`.

## Shared-helper ownership (extend these; never re-implement)

| Concern | Owner |
|---|---|
| Offset clamp, thumb metrics, cursor-follow, drag/track geometry | `jackin_tui::scroll` |
| Bordered passive scroll blocks, panel focus border, scrollbars | `jackin_tui::components::scrollable_panel` |
| Scrollable dialog bodies, per-axis overflow, scroll hints | `jackin_tui::components::dialog_layout` (`DialogBodyScroll`, `dialog_scroll_axes`, `scroll_hint_spans`, `render_scrollable_dialog_body`) |
| Debug-info rows, copy/hyperlink geometry | `jackin_tui::components::container_info` |
| Selectable rows, `▸` gutter, full-width highlight | `jackin_tui::components::select_list` / shared row helper |
| Panel chrome, body inset, title padding, focus border | `jackin_tui::components::panel` |
| `+ …` action rows | `action_row_style(selected)` in `jackin-console` |

## Source-of-truth references (read before editing the matching area)

| Reference | Governs |
|---|---|
| `docs/content/docs/reference/tui/components.mdx` | Reuse hard rule, component homes, settings/workspace parity |
| `docs/content/docs/reference/tui/architecture.mdx` | Elm boundaries, source locations, typed effects, render purity |
| `docs/content/docs/reference/tui/dialogs.mdx` | Modal sizing, five-slot padding, Debug-info contract, footer-only hints, modal click lifecycle |
| `docs/content/docs/reference/tui/chrome.mdx` | Bottom-chrome order, status/chip behavior, focus borders |
| `docs/content/docs/reference/tui/navigation.mdx` | W3C keyboard roles, hover/click affordances, cursor gutter, scroll hint/scrollbar coupling |
| `docs/content/docs/reference/tui/visual-design.mdx` | PHOSPHOR palette, default-bg fills, action-row style, copyable value styling |
| `docs/content/docs/reference/adrs/adr-003-ratatui.mdx` · `adr-004-pane-body-rendering.mdx` | Ratatui as the render library; `PaneBodyWidget` custom-body boundary |
| `crates/AGENTS.md` · `crates/jackin-tui/COMPONENTS.md` | Module layout, lint inheritance; component inventory |
| `docs/content/docs/reference/tui/lookbook/*.mdx` + `docs/public/tui-lookbook/*.svg` | Visual regression references |

## Goal checklist

Use this checklist as the phase-level operational map. The **Master ledger** remains the status source of truth; check an item only when the implementation, tests, docs, and ledger row agree.

### Phase 0 — Orient and protect the worktree

- [ ] Confirm branch is `feature/tui-architecture` and PR #495 is active.
- [ ] Review `git status --short`; preserve unrelated operator changes.
- [ ] Read this file's **Already landed** table before deleting or moving code.
- [ ] Read every source-of-truth TUI reference before editing matching TUI code.
- [ ] Search existing helpers/components before adding TUI code: `jackin-tui`, owning surface `src/tui`, and transitional root-console adapters.

### Phase 1 — Settle specs and docs first

- [ ] Resolve every item in **Spec gaps to resolve while implementing**.
- [ ] Keep canonical Debug-info label as `jackin version` unless a row intentionally changes all code/docs/tests/lookbook references.
- [x] Update docs where operator decisions supersede current docs, especially Debug-info backdrop and build-log click dismissal.
- [ ] Ensure roadmap status matches evidence: done, partial, deferred, or follow-up.
- [ ] Keep published docs free of stale PR-state claims; docs name current behavior, not intended future behavior.

### Phase 2 — Architecture cleanup

- [ ] Guard against reintroduced orphaned migrated source files.
- [ ] Verify extracted crates own their relevant tests where practical.
- [ ] Keep `jackin-diagnostics` free of `jackin-tui`.
- [x] Hoist duplicated per-crate lint policy into `[workspace.lints]` and opt crates in with `lints.workspace = true`.
- [ ] Reconcile documented closed-enum dispatch counts with actual code or update docs.
- [ ] Run targeted checks after structural cleanup so dead-path edits fail early.

### Phase 3 — Shared Debug info

- [ ] Audit every Debug-info entry point in console, launch, and capsule.
- [ ] Route every entry point through `DebugInfo` / `ContainerInfoState` / `render_container_info`.
- [ ] Remove parallel Debug-info renderers, row builders, copy behavior, hover behavior, hint generation, or scroll handling.
- [x] Fix launch hit-test data wiring so `Run ID` and `Diagnostics log` copy real values.
- [ ] Persist and clamp shared `DialogBodyScroll` state where surfaces rebuild dialog state each frame.
- [ ] Make horizontal/vertical scrolling, clipping, footer hints, hover, copy, copied feedback, and hyperlink overlays share the same geometry.
- [ ] Ensure `Run ID` and `Diagnostics log` are copyable everywhere they show copy affordances.
- [ ] Ensure Debug-info backdrop clears noisy content with default background while preserving reserved bottom chrome/status.
- [ ] Add cross-surface tests for row order, row labels, copy payloads, scroll axes, clipping, hover/click hit-testing, and backdrop where rows change behavior.

### Phase 4 — Scroll architecture and reuse

- [ ] Audit every scrollable component, dialog, overlay, pane, panel, list, and footer hint producer.
- [ ] Replace static scroll hints with `ScrollAxes` / `scroll_hint_spans` or equivalent shared overflow-derived state.
- [ ] Ensure no scrollbar appears when content fits and no scroll hint appears when the matching scrollbar is absent.
- [ ] Ensure renderer, input, hit-testing, drag, hover/copy overlays, resize clamps, and hints consume the same content extents and rect.
- [ ] Replace bespoke viewport, thumb, offset, and wheel math with `jackin_tui::scroll`, `scrollable_panel`, and `dialog_layout`.
- [ ] If capsule pane PTY cells cannot use `render_scrollable_block` directly, extract a reusable scrollable panel shell into `jackin-tui`.
- [ ] Add fit-content, horizontal-only, vertical-only, both-axes, resize, and max-scroll tests when touching scroll behavior.
- [ ] Add debug telemetry before behavior changes when current logs cannot prove render/input state in the same frame.

### Phase 5 — Capsule pane chrome and scrollback

- [ ] Preserve the PTY streaming body unless shared chrome/scroll correctness requires a body change.
- [ ] Replace capsule-specific pane border/focus palette with shared `Panel`/scrollable-panel green active/inactive behavior.
- [ ] Make pane title styling, body inset, border focus, scrollbar track/thumb, and focus transfer match Global mounts.
- [ ] Show pane scrollbars only on actual overflow.
- [ ] Make pane vertical scrollback monotonic and stable for wheel/touchpad bursts.
- [ ] Keep visible slice, scrollback offset, scrollbar thumb, cursor visibility, and footer hints derived from the same state.
- [ ] Verify alternate-screen panes do not flicker between live tail and retained scrollback while operator browses history.
- [ ] Add capsule render/input tests for long lines, repeated prompts with no overflow, scrollback, resize, and split panes.

### Phase 6 — Dialogs, rows, and click targets

- [ ] Render `Git repository detected` with canonical five-slot dialog layout.
- [ ] Fix file-browser parent gutter so hiding `▸` behind a child dialog does not shift row text.
- [ ] Fix Auth source/source-folder rows so every selectable row reserves the cursor gutter consistently (`PRE-3`: Settings and workspace editor are forked paths; fix both).
- [ ] Make every `+ ...` creation sentinel use the same action-row color, weight, selected effect, and cursor-gutter behavior.
- [ ] Fix `ErrorDialog` spacing in the shared component, not at one caller.
- [ ] Update lookbook stories/SVGs for changed shared dialog or panel output.
- [x] Ensure inside clicks on build-log overlay are swallowed unless they hit a real target; close only with `Esc`/`q`.
- [ ] Ensure all clickable targets have distinct resting style, hover lift, pointer routing where supported, and click-to-action tests.

### Phase 7 — Verification and closeout

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- [ ] Run `cargo nextest run --workspace --all-features`.
- [ ] Run docs checks from `docs/`: `bun run build`, `bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test`.
- [ ] Regenerate/check lookbook SVGs when shared component visuals change.
- [ ] Run or inspect `gh pr checks 495`.
- [ ] Update checklist statuses and ledger rows so remaining work is explicit.
- [ ] If anything is deferred, create/update roadmap follow-up and state exact files/behaviors left.
- [ ] Produce the completion report.

## Spec gaps to resolve while implementing

| Gap | Required resolution |
|---|---|
| Debug-info backdrop wording differs across docs and operator expectation | Done in `dialogs.mdx` + `chrome.mdx`: modal body/background hidden by default-background backdrop; reserved bottom chrome/status remains visible. Verified with `cd docs && bun run build`. |
| Debug-info version row naming has historically drifted (`jackin version` vs `jackin`) | Keep canonical `jackin version` across `DebugInfo::into_state()`, tests, docs, and lookbook unless a row intentionally changes every reference. |
| Build-log overlay docs mention click dismissal, but desired behavior is keyboard close only | Done: `chrome.mdx` says `Esc`/`q` close; body clicks are swallowed; scrollbar clicks remain interactive. Launch subscriptions match this rule. |
| Capsule pane chrome currently has a capsule-specific palette | Replace or wrap it with shared panel/focus/scrollbar palette used by Global mounts. If PTY cells need a lower-level shell, define it as reusable `jackin-tui` primitive and document it. |
| Scroll hint producers are scattered | Collapse producers onto `ScrollAxes` / `scroll_hint_spans` or equivalent shared panel overflow state. Any remaining static scroll hint must be justified by visible overflow in the same render path. |
| Settings and workspace editor Auth rows may be separate render paths | Audit both. A fix in one path that leaves the other drifting violates settings/editor parity. |

## Operator notes

These notes preserve the detailed old fix-plan findings. When a ledger row completes one of these notes, update the note status in the same commit.

| Status | Area | Summary | Decision / constraint | Ledger |
|---|---|---|---|---|
| pending | TUI / Debug info | Every Debug-info display must use the same component and behavior on every screen | Screens may provide more/fewer facts; shared component owns ordering, labels, copy affordances, scrolling, hints, rendering | `DBG-1`, `DBG-2`, `DBG-3` |
| done | TUI / Debug info | Launch Debug-info version row showed diagnostics JSONL path | Each row displays only its own fact; unknown version row is omitted | Already landed; guard in `DBG-1` |
| done | TUI / Debug info | Horizontal scroll clipped long values outside dialog body | Scrolled content stays clipped to dialog inner area | Already landed |
| done | TUI / Debug info | `Run ID` and `Diagnostics log` copy affordances are real on launch hit-test path | Copy affordance means hoverable, clickable, clipboard write, copied feedback | `DBG-1`; `container_info_click_copies_real_run_id_and_log_path` |
| done | TUI / Debug info | Launch/capsule background content showed behind Debug-info | Debug-info paints default-background backdrop over body, not bottom chrome | Already landed; docs in `PRE-1` |
| done | TUI / Hints footer | Debug-info hints rendered as floating row under dialog | Modal hints live only in reserved footer/hint rows | `DBG-2`; footer-row regression test |
| pending | TUI / Hints footer | Scroll hints can show when no scroll is possible | Hints derive from same overflow gate as scrollbar | `SCR-1`, `SCR-2`, `SCR-3` |
| done | TUI / Debug info | Capsule Debug-info hover/click may be off by one | Render rect, hover rect, click rect, copy feedback all use same geometry | `DBG-3`; rendered-cell coordinate tests |
| pending | TUI / Capsule panes | Pane scrollbar/thumb can disagree with visible content | Content height, viewport, offset, thumb derive from one shared state model | `CAP-2`, `CAP-3` |
| pending | TUI / Capsule panes | Pane vertical scroll can flicker/reverse under wheel bursts | One input direction produces one visible direction; live PTY output must not fight retained view | `CAP-2` |
| done | TUI / Capsule panes | Pane scrollbar showed when content fit | Scrollbar is overflow affordance only | Already landed; guard in `SCR-1`, `CAP-3` |
| pending | TUI / Capsule panes | Pane chrome and scrollbar do not match Global mounts | Reuse shared panel/block chrome around custom PTY body | `CAP-1`, `CAP-3`, `RMP-5` |
| done | TUI / Build log overlay | Build-log overlay close semantics now have doc/code parity | Body click swallowed; `Esc`/`q` close; scrollbar remains interactive | `PRE-2`; `build_log_body_click_is_swallowed`; docs build |
| pending | TUI / Dialog layout | `Git repository detected` prompt has wrong top padding | Content plus buttons uses canonical five-slot layout | `DLG-1` |
| pending | TUI / File browser | Child git prompt collapses parent file-browser gutter | Hiding `▸` never shifts row text | `DLG-2` |
| pending | TUI / Auth editor | Auth source rows do not reserve cursor gutter consistently; Settings and workspace editor use forked render paths | All selectable Auth rows reserve same two-cell gutter; `DLG-3` must fix both `settings/view.rs` and `editor/view.rs` or unify the helpers | `PRE-3`, `DLG-3` |
| pending | TUI / Action rows | `+ New workspace` does not match `+ Add mount` | All `+ ...` creation sentinels use one action-row style | `DLG-4` |
| pending | TUI / Error dialog | `Load role failed` has two blank rows before `OK` | Shared `ErrorDialog` owns one content-to-action spacer | `DLG-5` |
| deferred | Perf / Roadmap | Terminal performance claims need measured support | If measurements are deferred, roadmap says partial/deferred plainly | `RMP-3`, `RMP-4` |

## Preserved decisions

Use these decisions when a row detail and older audit language appear to conflict:

- **Debug-info is one shared component.** Surfaces may gather different facts, but the shared component owns ordering, labels, copy affordances, hover/click geometry, scroll, hints, clipping, copied feedback, and hyperlink layout. Unknown facts are omitted, not filled with unrelated placeholders.
- **Debug-info backdrop hides body content.** Modal body/background content is cleared to terminal default background. Reserved bottom chrome/status remains visible and is rendered by its normal owner.
- **Build-log close is keyboard-only.** `Esc`/`q` close. Inside body clicks are swallowed unless they hit a real target such as a scrollbar.
- **Scrollable behavior is shared end-to-end.** Renderer, input, hit-testing, hover/copy overlays, scrollbars, resize clamps, and footer hints consume the same content extents, viewport rect, and clamped offsets.
- **Capsule PTY body is allowed to stay custom.** ADR-004 permits `PaneBodyWidget` for terminal cells. Pane chrome, focus border, scrollbar metrics, overflow gates, and hints still come from shared `jackin-tui` primitives or a newly extracted reusable shell.
- **Creation sentinel rows are one pattern.** `+ New workspace`, `+ Add mount`, `+ Override for a role`, and every other `+ ...` row use one action-row style and stable cursor gutter.
- **Roadmap honesty beats optimistic checkboxes.** If code is partial, unmeasured, or deferred, roadmap status says so with exact remaining scope.

## Execution strategy

Use this order unless live evidence proves a dependency points elsewhere:

1. Stabilize spec and references: settle spec gaps, update docs/lookbook references as implementation decisions become final.
2. Finish architecture cleanup: lint adoption and console extraction before editing paths that may move.
3. Unify Debug info: one shared model/renderer/state path; fix data wiring, backdrop, footer hints, scroll, hover, copy, hit-testing together.
4. Unify scroll primitives: scrollbar visibility, hint axes, input routing, hit tests, clipping all consume same overflow facts.
5. Fix capsule pane chrome around PTY body: preserve stream body; move reusable shell/chrome pieces into `jackin-tui` if needed.
6. Normalize dialog and selectable-row layout: five-slot padding and stable cursor gutters through shared helpers.
7. Update lookbook and snapshots with shared component visual changes.
8. Run full verification; do not mark rows complete until code, tests, docs, and visual references agree.

## Verification gates

Run before merge request / ready-for-review:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features
```

For docs changes, run from `docs/`:

```sh
bun run build
bun run check:repo-links
bunx tsc --noEmit
bun test
```

Also inspect:

```sh
gh pr checks 495
```

---

# Phase 0 — Preflight & spec gaps

Orient and settle the spec contradictions later phases depend on. Almost no code changes here.

**`ARCH-0`** — Orphan-tree deletion and the `diagnostics→tui` removal already landed (see Already landed). Action is a no-op verification: confirm the trees are still gone and diagnostics has no `jackin-tui` dep. If a rebase reintroduces a shadowed `runtime/`/`isolation/` child, delete it; keep only the re-export shim.

**`PRE-1`** *(done)* — `dialogs.mdx` and `chrome.mdx` now say the same thing: Debug info clears modal body/background content with an opaque default-background backdrop inside the content area, while reserved bottom chrome/status stays visible and remains owned by its normal renderer. Verified with `cd docs && bun run build`.

**`PRE-2`** *(done)* — Intended build-log rule is keyboard-only close (`Esc`/`q`); an inside body click is a no-op unless it hits the scrollbar. The launch code still mapped ordinary body clicks to `BuildLogClosed`, so the behavior was fixed instead of documenting a future rule: `crates/jackin-launch/src/tui/subscriptions.rs` now swallows plain body clicks, keeps scrollbar track/thumb clicks interactive, and has `build_log_body_click_is_swallowed`. `chrome.mdx` and the build-log component docs now match. Verified with `cargo nextest run -p jackin-launch build_log` and docs build.

**`PRE-3`** *(done)* — Read both Auth renderers. Settings Auth lives in `crates/jackin-console/src/tui/screens/settings/view.rs` (`render_auth_source_line`, `render_auth_source_folder_line`). Workspace-editor Auth is a separate fork in `crates/jackin-console/src/tui/screens/editor/view.rs` (`render_auth_source_line`, `render_source_folder_line`, `editor_auth_line_width`). The editor workspace source/source-folder rows pass `indent = 0` and render no cursor gutter, while adjacent workspace mode and `+ Override for a role` rows reserve `▸ ` / `  `. Settings source/source-folder rows reserve the two-cell cursor gutter, but Kind renders as one styled span (`"{cursor_col}{label}"`) rather than through the same row helper. `DLG-3` must fix both files or unify the helpers; a Settings-only fix would leave editor Auth drifting.

---

# Phase 1 — Architecture cleanup

The big audit items (orphan deletion, `diagnostics→tui`) already landed (`ARCH-0`). What remains is lint adoption and finishing the console extraction.

**`ARCH-1`** *(done)* — The policy table lives at root (`Cargo.toml`: `[workspace.lints.rust]` + `[workspace.lints.clippy]`, incl. `mod_module_files`, `unwrap_used`, `expect_used`, `print_stdout/stderr` deny, the clippy `all`/`pedantic`/`cargo` groups). All 17 `crates/*/Cargo.toml` manifests now contain `[lints]\nworkspace = true`; no private `[lints.clippy]` / `[lints.rust]` crate tables remain. `crates/AGENTS.md` documents lint inheritance and suppression discipline. Verified opt-in count = 17 and `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passed.

**`ARCH-2`** *(done)* — The "~17 arms" undercount was in the roadmap checklist, not `architecture.mdx`, and is already corrected: `post-restructure-fixes-checklist.mdx` and `agent-runtime-trait.mdx` state the verified 58 production `Agent::Variant =>` / `Provider::Variant =>` arms across 6 files, with the per-file breakdown. Optional residual (collapse/justify the `agent_binary.rs` + `multiplexer_utils.rs` exception arms) → `RMP-6`.

**`ARCH-3`** *(deferred)* — The launch half is done (`crates/jackin-launch` owns the launch model/view; the `runtime/progress.rs` facade is gone). The remaining console extraction is too large to finish safely inside this row without rewriting the live manager boundary: current branch evidence shows `crates/jackin/src/console/` still contains 93 Rust files / 39,292 LOC. Exact remaining root modules: `manager.rs`, `domain.rs` + `domain/tests.rs`, `services.rs` + `services/{agents,browser,config,file_browser,instances,op,op_picker,role_load,token_setup,workspace_save}.rs`, `effects.rs`, `terminal.rs`, `preview.rs`, `outcome.rs`, `widgets.rs`, and root TUI adapters under `tui/{app,components,debug,effect,input,instance_action,launch,layout,message,op_picker,prompts,run,state,view}.rs` plus their child modules/tests. The roadmap/codebase map now treats this as **Partially implemented** live root-integration work, not duplicate dead code, and names future extraction targets (`jackin-console` or smaller lower-tier crates when dependency direction stays acyclic). Verified current boundary with `cargo build -p jackin-console -p jackin-launch`.

---

# Phase 2 — Shared Debug info

Most Debug-info findings already landed (see Already landed). Read `dialogs.mdx` (Debug-info contract) first. Canonical path for all surfaces: `DebugInfo` → `ContainerInfoState` → `render_container_info`; screens differ only in which facts they gather.

**`DBG-1`** *(done)* — The render path already threaded the correct `run_log_path`, but the mouse/key hit-test path rebuilt `ContainerInfoState` with `""`, making `Diagnostics log` copy an empty payload. `handle_cockpit_input` now receives the real `run_log_path`, carries it with the run id in a `CockpitContext`, and every launch Debug-info state builder for click, hover, wheel/key scroll, clamp, and Enter-copy uses the same values as render. Regression test `container_info_click_copies_real_run_id_and_log_path` clicks both visible rows and asserts the copied payloads are the bare run id and full JSONL path. Verified with `cargo nextest run -p jackin-launch` and `cargo clippy -p jackin-launch --all-targets --all-features --locked -- -D warnings`.

**`DBG-2`** *(done)* — Launch Debug-info no longer paints a floating hint row under the dialog. `render_launch_container_info` reserves the standard bottom chrome via `bottom_chrome_areas`, clears the body backdrop only, renders hint spans into the fixed hint row, clears the separator row, and re-renders the status footer. `launch_container_info_rect` centers the dialog inside the body area so rendering, hit-testing, scroll axes, and overlays share the same bottom-chrome-aware geometry. The old shared `render_debug_info_hint` helper was deleted so new surfaces cannot accidentally use the floating placement again. Regression test `launch_debug_info_keeps_status_footer_visible` asserts `copy value` / `Esc` live in the footer hint row, the separator remains blank, and no hint text appears immediately below the dialog. Verified with `cargo nextest run -p jackin-tui -p jackin-launch -p jackin-capsule`, `cargo clippy -p jackin-tui -p jackin-launch -p jackin-capsule --all-targets --all-features --locked -- -D warnings`, and `cargo fmt --check`.

**`DBG-3`** *(done)* — Capsule Debug-info geometry now has direct rendered-cell coverage for the off-by-one class. `container_info_visible_debug_rows_map_to_shared_hit_targets` renders the shared Debug-info state, locates visible `Run ID`, `Container ID`, and `Diagnostics log` value cells, and asserts the same screen coordinates hit the matching shared copy payload. `container_info_visible_container_row_maps_to_dialog_hover_and_copy_target` runs the capsule dialog wrapper path for the visible container-id cell and asserts hover row, copied payload, and copied-row feedback all target the same rendered row. This proves render geometry, hit-test geometry, hover state, click copy, and copied feedback stay aligned for capsule Debug-info rows. Verified with `cargo nextest run -p jackin-capsule`, `cargo clippy -p jackin-capsule --all-targets --all-features --locked -- -D warnings`, and `cargo fmt --check`.

---

# Phase 3 — Overflow-derived scroll hints

Global rule (`navigation.mdx`): a scroll hint appears only when that axis overflows and its scrollbar is visible. The correct shared impl already exists — `scroll_hint_spans` / `dialog_scroll_axes` / `ScrollAxes` in `dialog_layout.rs`; launch dialogs already use it. Two surfaces still emit static hints.

**`SCR-1`** — `crates/jackin-capsule/src/tui/components/dialog/hint.rs:15-44` — `MAIN_VIEW_HINT` / `SCROLLBACK_HINT` are constants that always advertise `↑↓`, gated only on a `scrollback_active` bool. The capsule already has an axis-derived helper for its info dialog (`info_dialog_hint(axes)` at `:88`); extend the same pattern to the main/scrollback view. Derive vertical overflow from the focused pane's retained content height vs viewport height (the same gate as `CAP-3`), horizontal from content vs viewport width. Fit-content pane → no hint. Add a fit-content test.

**`SCR-2`** — `crates/jackin-console/src/tui/components/footer_hints.rs:150-193` gates the workspace block hint on focus bools (`scroll_focused`, `show_horizontal_scroll`); `:374` hardcodes the trust-row `H/L scroll`. Route both through real per-axis overflow. Tests: fit-content (no hint), vertical-only, horizontal-only, both.

**`SCR-3`** — Enumerate every `scroll_hint_spans` / `ScrollAxes` call site and every literal `"scroll"` hint string across `crates/*/src`. Produce an audit table (producer → axis-derived? → action). No ungated static `↑↓`/`←→`/`H/L scroll` string may survive.

---

# Phase 4 — Capsule pane chrome & scrollback

Reference look = the **Global mounts** scrollable block. The PTY body (`PaneBodyWidget`, ADR-004) stays custom — only surrounding chrome, focus palette, scrollbar, and scrollback state are in scope. Read `adr-004` + `visual-design.mdx` first. The scrollbar-overflow gate already landed.

**`CAP-1`** — `crates/jackin-capsule/src/tui/components/chrome.rs:189` — `PaneBorderWidget` uses `FocusPalette::CAPSULE_PANE` (gray ramp, `panel.rs:59-62`) instead of the shared `PHOSPHOR_GREEN` active/inactive border. Route pane border/title through `Panel` (or a shared wrapper) using the standard green; reuse Global-mounts focus-transfer (click/wheel over a scrollable pane focuses it, previous pane loses its border same frame; non-scrollable panes show no focused-scroll state). If `render_scrollable_block` cannot paint terminal cells, extract a reusable **scrollable-panel shell** into `jackin-tui` that owns chrome/focus/scrollbar/hints and accepts `PaneBodyWidget` as the body — preferred shape. Do not change the PTY stream body. Render test compares pane border colour + title + thumb glyphs against the shared helper, incl. split panes.

**`CAP-2`** — Vertical scrollback is non-monotonic / flickers. Confirmed mechanisms: `input_dispatch.rs:363,419` (`scroll_by` on `filled>0`); `compositor.rs:161-162` (`filled=0` for alt-screen); `session.rs:846-850,694` (`feed_pty` → `scroll_to_live` resets offset); `compositor.rs:584` (`append_cursor_state` keyed on `scrollback_offset!=0`). **Telemetry first** (project rule): add `cdebug!` logging focused pane id, agent label, `alternate_screen`, content/scrollback length, viewport rows/cols, tail offset the renderer used, thumb start/len, visible top row, cursor-visibility decision — all in one frame. Ask the operator to rerun with `--debug` and share the run id; fix from that. Then normalize the wheel path: decode SGR buttons to typed direction/axis; ignore horizontal wheel for vertical scrollback unless a real horizontal path exists; coalesce wheel events per frame into one signed delta; apply through one shared tail-scroll helper and clamp once; keep slice + thumb + hint + cursor on that one post-clamp offset; do not reset the operator's view on live PTY output unless content is invalidated. Tests: same-direction burst → monotonic top row + offset; interleaved PTY while scrolled → no flicker to tail.

**`CAP-3`** *(partial)* — `crates/jackin-capsule/src/tui/view.rs:178` already uses `jackin_tui::scroll::tail_vertical_thumb` for geometry, but `:192-197` hand-paints the thumb cells. Either accept the custom paint (reasonable for the terminal-cell shell) with a one-line comment naming why, or fold it into the `CAP-1` shell extraction. Decide and document — do not leave it ambiguous.

---

# Phase 5 — Dialogs, rows & click targets

All five confirmed real at HEAD. Each fix belongs in a shared helper, not at one caller. Read `dialogs.mdx` (five-slot padding, modal lifecycle) + `visual-design.mdx` (action-row, cursor gutter) first.

**`DLG-1`** — `git_prompt.rs:147` (`git_prompt_rect` = 8/7) vs `:249` (`render_git_prompt` = 7/6); render hand-rolls constraints instead of `dialog_inner_chunks`. Render with `dialog_inner_chunks(inner, Some(content_rows))`: leading spacer, content (prompt + optional URL), spacer, action row, trailing spacer. Reconcile so `git_prompt_rect` height == render height. Test: blank row below top border; prompt on next row; buttons separated by one spacer; URL click rect points at the URL row.

**`DLG-2`** — `file_browser/render.rs:104` (`show_cursor = pending_git_prompt.is_none()`) + `:140-142` (`highlight_symbol` only set when `show_cursor`) → gutter collapses when the child dialog opens, despite `HighlightSpacing::Always`. Keep the two-cell symbol width reserved always (render a blank symbol when suppressed). Parent may dim its border / drop the active marker, but not move text. Extend `git_prompt_background_suppresses_browser_cursor_and_active_border`.

**`DLG-3`** — `settings/view.rs` and `editor/view.rs` use forked Auth renderers. Settings: Kind uses a fused `"{cursor_col}{label}"` span while Mode/Source/Source-folder reserve a two-cell gutter. Editor: workspace Mode and `+ Override for a role` reserve `▸ ` / `  `, but workspace Source/Source-folder call local helpers with `indent = 0`, so they reserve no cursor gutter; role rows use `indent = 6` and are another local shape. Route Settings and workspace-editor Auth source/source-folder rows, Kind/Mode rows, and the `+ Override for a role` sentinel through the same two-cell cursor-gutter helper (or unify the renderers outright). Acceptance: selected shows `▸`, unselected `  `; label start column identical for `Mode`, `Source`/`Source folder`, `+ Override for a role` in both Settings and workspace editor.

**`DLG-4`** — `workspaces/view.rs:77-86` (`new_workspace_display_row`, tone `White`) + `:286-348` (`push_tree_workspace_line` hardcodes `"{cursor}  {label}"`); `+ Add mount` correctly uses `action_row_style` (`settings/view.rs:637`, `editor/view.rs:430`). Route `+ New workspace` through `action_row_style` (extend it if it cannot own row construction + gutter + selected state). Sweep all `+ ` rows. Snapshot-compare `+ New workspace` vs `+ Add mount` selected + unselected.

**`DLG-5`** — `crates/jackin-tui/src/components/error_dialog.rs:65` — `body_rows = inner.height.saturating_sub(4)` gives all remaining inner height to the body, so short messages get >1 blank row before `OK`. Fix in the shared component: size the body slot from estimated wrapped message rows (capped), reserve exactly leading spacer (1) / body / spacer (1) / `OK` (1) / trailing spacer (1). Keep wrapping/scroll for overflow. Test: exactly one blank row between the last message line and `OK`; lookbook `error/default` uses the shared component (regenerate its SVG).

---

# Phase 6 — Roadmap status reconcile & deferred work

Surfaced by the audit (former `PR-495-REVIEW.md`, Part 2 A/C/E). Several roadmap acceptance items are `[x]` while their notes say deferred/partial/unmeasured. None are code merge blockers — but the **roadmap-freshness hard rule** makes honest re-statusing a pre-merge requirement. For each: confirm code state, then complete (only if cheap + in scope) or re-status the roadmap item honestly with exact remaining scope. Run the `docs/AGENTS.md` sidebar + overview audits after any status/file move.

**`RMP-1`** — Diagnostics JSONL is written directly; `tracing` additive (`crates/jackin-diagnostics/src/run.rs`); roadmap claimed "span-sourced". Build the inversion (spans authoritative, a `JackinDiagnosticsLayer` emits the JSONL) or correct the status to "JSONL direct, tracing additive, `span_id` only".

**`RMP-2`** *(deferred)* — Observability metrics surface (stage-duration histograms + cache hit/miss counters) not built; only a `duration_ms` field added. Re-status the roadmap item Planned/Partial.

**`RMP-3`** *(deferred)* — `jackin-term` zero-alloc tail (PageList arena, `RefCountedSet` interning, multi-session slab, `dirty_spans()` emit integration) deferred; `Vec<Vec<Cell>>` still allocates. Re-state the zero-alloc acceptance as partial; complete only if `present_frame`/`dhat` numbers justify it.

**`RMP-4`** *(deferred)* — Real PTY conformance corpus (`claude`/`codex`/`vim`/`htop`/asciinema) absent; differential harness runs inline fixtures only. Re-status as outstanding.

**`RMP-5`** — Capsule chrome still emits VT100/ANSI rather than `jackin-tui` primitives — the largest remaining "two implementations" risk. `CAP-1` migrates the pane border palette; the broad chrome migration is bigger. Ensure a named roadmap item tracks "capsule ANSI→Ratatui" with remaining scope; cross-link `CAP-1`/`CAP-3`.

**`RMP-6`** — Stale roadmap acceptance notes (audit Part 2 A / Part 4): "Green everywhere", `fmt`, `nextest`, "clippy blocked by capsule test compile" were `[x]` while gates were red. Those gates are green at HEAD; the notes still mislead. Update the acceptance lines; re-state still-deferred `[x]` as `[~]`; collapse or `#[expect]`-justify the `agent_binary.rs` + `multiplexer_utils.rs` exception arms (the `ARCH-2` residual).

**`RMP-7`** *(deferred, optional)* — God files have shrunk but remain large: `console/tui/input/global_mounts.rs` 1407, `capsule/.../dialog.rs` 1425, `console/.../op_picker.rs` 1197 LOC. "Not urgent." Split along input/state/render seams when next touching them.

---

# Phase 7 — Verify & closeout

Cargo gates are green at HEAD; the live merge blockers are in docs CI. From `gh pr checks 495`: green incl. `cargo fmt`/`clippy`/`nextest`/`DCO`/`amd64`/`arm64`/`cargo audit`/`repo-link-check`/`docs-link-check`. **Failing:** `spell-check-docs`, `docs-required`.

**`CI-1`** — Find what `spell-check-docs` flags (`gh run view --log-failed --job <id>`). Correct real typos in `docs/` or add legitimate technical terms (brand words, crate/agent names) to the dictionary. Note: brand prose uses `jackin'` (apostrophe); literal identifiers use `jackin`. The job scans `.github/**/*.md`, `docs/**/*.md(x)` (`.github/workflows/docs.yml:189-192`) — not the root `PR-495-GOAL.md`.

**`CI-2`** — `docs-required` is a path-aware roll-up; it goes green when its underlying docs jobs (spell-check, build, link, type) pass. Confirm every doc touched by phases 0–6 (`dialogs.mdx`, `chrome.mdx`, `navigation.mdx`, `crates/AGENTS.md`, roadmap pages, lookbook stories) builds and links.

**`CI-3`** — Run the four `bun` docs gates locally before pushing; fix locally rather than discovering failures in CI.

**`CI-4`** *(done — keep green)* — After each task's edits re-run `cargo fmt --check`, `clippy … -D warnings`, `cargo nextest run --workspace --all-features`. `ARCH-1` is the most likely to surface new clippy findings — budget for it.

## Completion report

- Each ledger row's final status; any `pending`/`deferred` with exact reason and remaining files/behaviors.
- Shared components/helpers changed or added (expected: `container_info` hint placement, `ErrorDialog` sizing, `action_row_style` reach, a possible `jackin-tui` scrollable-panel shell, `[workspace.lints]` adoption).
- Docs + lookbook artifacts updated; roadmap items moved per the freshness rule.
- Verification commands run and results.
- Residual risk — especially `CAP-2` (capsule vertical scrollback) and `DBG-3` if it stayed smoke-only.
