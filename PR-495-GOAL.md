# PR #495 — Goal

- **PR:** https://github.com/jackin-project/jackin/pull/495 (`refactor: finish TUI architecture epic`, DRAFT → `main`)
- **Branch:** `feature/tui-architecture`
- **Code verified at HEAD `f920b29a`** (doc-only commits since). Every status in the **Master ledger** was checked against the live tree, not inherited from an audit.

This file supersedes the earlier PR #495 fix plan and review-note queue. It is the single working spec for finishing PR #495: one master status ledger, one detail section per phase, one source of truth for status. The ledger is the only place status lives. Disposable operator doc at the repo root (outside the `docs/**` / `.github/**` CI globs); delete after merge.

## Operating contract

1. Read this file once in this order: **Operating contract**, **Master ledger**, **Already landed**, **Non-negotiable TUI invariants**, **Shared-helper ownership**, **Source-of-truth references**, then only the phase sections needed for the current row.
2. Work the **Master ledger** top to bottom. Skip `done` rows and skip **Already landed**. Verify evidence before acting; stale evidence gets corrected in the row.
3. Keep exactly one row `in_progress`. No phase-level status, no second checklist, no shadow notes.
4. On finishing a row: update its ledger status, add any new evidence to the phase detail, run the row's verify command, then commit + push. This makes the ledger resumable across context resets.
5. Status vocabulary: `pending`, `in_progress`, `done`, `deferred`. Nothing else. `done` = code changed + tests/docs/lookbook updated where required + verify passed or failure documented. `deferred` = named roadmap/follow-up with exact remaining files/behaviors.
6. No silent scope shrink. If investigation proves a row is obsolete, mark it `done` with evidence; if it is too large, mark it `deferred` with a concrete follow-up. Do not delete rows to make progress look cleaner.
7. No local styling forks. A fix that adds a second colour, border, hint, row, scroll, or click style is wrong; extend the shared helper named by the row. Docs land with code: if a rule changes, update the matching `docs/content/docs/reference/tui/*.mdx` and roadmap status in the same commit.
8. If blocked by missing live evidence, add durable `cdebug!` telemetry first, record the needed rerun/run id in the row, then stop. End of run -> produce the **Completion report**.

## Master ledger

Verify each row's evidence before acting — if it now reads as already handled, mark it `done` with new evidence and move on.

| ID | Phase | Status | Task | Verify |
|---|---|---|---|---|
| `ARCH-0` | 0 Preflight | done | Orphan trees + `diagnostics→tui` dep already removed; guard against regression | `cargo check -p jackin -p jackin-diagnostics` |
| `PRE-1` | 0 Preflight | pending | Reconcile Debug-info backdrop wording across `dialogs.mdx` + `chrome.mdx` | docs build |
| `PRE-2` | 0 Preflight | pending | Settle build-log close semantics in `chrome.mdx`; file any code task found | docs build |
| `PRE-3` | 0 Preflight | pending | Audit Settings vs workspace-editor Auth render paths; feed `DLG-3` | read-only |
| `ARCH-1` | 1 Architecture | pending | Adopt `[workspace.lints]` in all 17 crates; delete private tables | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| `ARCH-2` | 1 Architecture | done | Dispatch-arm count already corrected to 58 in roadmap docs | docs build |
| `ARCH-3` | 1 Architecture | pending | Finish moving the manager loop out of root `src/console/` into `jackin-console` | `cargo build -p jackin-console -p jackin-launch` |
| `DBG-1` | 2 Debug info | pending | Launch copy passes empty `run_log_path` to hit-test state | `cargo nextest run -p jackin-launch` |
| `DBG-2` | 2 Debug info | pending | Debug-info hint floats below dialog instead of fixed footer | `cargo nextest run -p jackin-tui -p jackin-launch -p jackin-capsule` |
| `DBG-3` | 2 Debug info | pending | Capsule Debug-info hover off-by-one — confirm by live smoke | smoke + `cargo nextest run -p jackin-capsule` |
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
- **One scroll geometry.** Renderer, input, hit-testing, scrollbars, hover/copy overlays, and hints derive from the same content extents, viewport rect, and clamped offsets — via the shared helpers below.
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

## Preserved decisions

Use these decisions when a row detail and older audit language appear to conflict:

- **Debug-info is one shared component.** Surfaces may gather different facts, but the shared component owns ordering, labels, copy affordances, hover/click geometry, scroll, hints, clipping, copied feedback, and hyperlink layout. Unknown facts are omitted, not filled with unrelated placeholders.
- **Debug-info backdrop hides body content.** Modal body/background content is cleared to terminal default background. Reserved bottom chrome/status remains visible and is rendered by its normal owner.
- **Build-log close is keyboard-only.** `Esc`/`q` close. Inside body clicks are swallowed unless they hit a real target such as a scrollbar.
- **Scrollable behavior is shared end-to-end.** Renderer, input, hit-testing, hover/copy overlays, scrollbars, resize clamps, and footer hints consume the same content extents, viewport rect, and clamped offsets.
- **Capsule PTY body is allowed to stay custom.** ADR-004 permits `PaneBodyWidget` for terminal cells. Pane chrome, focus border, scrollbar metrics, overflow gates, and hints still come from shared `jackin-tui` primitives or a newly extracted reusable shell.
- **Creation sentinel rows are one pattern.** `+ New workspace`, `+ Add mount`, `+ Override for a role`, and every other `+ ...` row use one action-row style and stable cursor gutter.
- **Roadmap honesty beats optimistic checkboxes.** If code is partial, unmeasured, or deferred, roadmap status says so with exact remaining scope.

---

# Phase 0 — Preflight & spec gaps

Orient and settle the spec contradictions later phases depend on. Almost no code changes here.

**`ARCH-0`** — Orphan-tree deletion and the `diagnostics→tui` removal already landed (see Already landed). Action is a no-op verification: confirm the trees are still gone and diagnostics has no `jackin-tui` dep. If a rebase reintroduces a shadowed `runtime/`/`isolation/` child, delete it; keep only the re-export shim.

**`PRE-1`** — `dialogs.mdx` and `chrome.mdx` historically described the Debug-info backdrop differently. Shipped behavior (verified: launch clears the area with `Clear`, then preserves bottom chrome) is the stricter rule. Make both pages say exactly that: modal body/background hidden by a default-background backdrop; reserved bottom chrome/status stays visible. `DBG-2` acceptance leans on this.

**`PRE-2`** — Intended build-log rule is keyboard-only close (`Esc`/`q`); an inside body click is a no-op unless it hits the scrollbar. Before editing the doc, re-check the launch code at HEAD (`crates/jackin-launch/src/tui/subscriptions.rs` — the audit flagged ordinary overlay clicks mapping to `BuildLogClosed`). If the code still closes on inside click, file a new `DLG-`style task in Phase 5 — do not let the doc claim a behavior the code lacks.

**`PRE-3`** — Read both Auth renderers. `crates/jackin-console/src/tui/screens/settings/view.rs` owns Settings Auth (`render_auth_source_line`, `render_auth_source_folder_line`). Find the workspace-editor Auth renderer; determine whether it shares those functions or forks them. Write the finding into `DLG-3` before implementing the gutter fix — a fix on one path that leaves the other drifting violates settings/editor parity.

---

# Phase 1 — Architecture cleanup

The big audit items (orphan deletion, `diagnostics→tui`) already landed (`ARCH-0`). What remains is lint adoption and finishing the console extraction.

**`ARCH-1`** — The policy table already lives at root (`Cargo.toml:60+`: `[workspace.lints]` + `[workspace.lints.clippy]`, incl. `mod_module_files`, `unwrap_used`, `expect_used`, `print_stdout/stderr` deny, the clippy `all`/`pedantic`/`cargo` groups). The fix is **adoption**: for each of the 17 `crates/*/Cargo.toml`, add `[lints]\nworkspace = true` and delete the private `[lints]`/`[lints.clippy]` table. Keep only documented one-line exceptions (with a comment naming why). Re-run clippy; fix or scoped-`#[allow]` any newly surfaced lints (no broad crate-level allow). Update `crates/AGENTS.md` so the documented guarantee matches the real mechanism. Verify: `rg -l "lints.workspace = true" crates/*/Cargo.toml | wc -l` == 17.

**`ARCH-2`** *(done)* — The "~17 arms" undercount was in the roadmap checklist, not `architecture.mdx`, and is already corrected: `post-restructure-fixes-checklist.mdx` and `agent-runtime-trait.mdx` state the verified 58 production `Agent::Variant =>` / `Provider::Variant =>` arms across 6 files, with the per-file breakdown. Optional residual (collapse/justify the `agent_binary.rs` + `multiplexer_utils.rs` exception arms) → `RMP-6`.

**`ARCH-3`** — The launch half is done (`crates/jackin-launch` owns the launch model/view; the `runtime/progress.rs` facade is gone). The console half is not: root `crates/jackin/src/console/` still holds the manager loop (`manager.rs`, `domain/`, `services/`, `tui/`, `effects.rs`, 472-line `console.rs`). Finish it: (1) move the remaining manager loop, screen state, and render/input modules into `crates/jackin-console` with per-screen state/update/tui modules; (2) leave root `jackin` only the thin CLI/runtime routing (or remove `src/console/`); (3) keep each surface's Elm boundary intact; (4) update roadmap `tui-architecture.mdx` Phase 10 status + sidebar/index in the same change. This was TODO.md `jackin-console-jackin-launch-extraction`. Scope to what reasonably lands in PR #495 — if it cannot fully complete, mark `deferred` with the exact remaining modules and keep the roadmap item **Partially implemented**.

---

# Phase 2 — Shared Debug info

Most Debug-info findings already landed (see Already landed). Read `dialogs.mdx` (Debug-info contract) first. Canonical path for all surfaces: `DebugInfo` → `ContainerInfoState` → `render_container_info`; screens differ only in which facts they gather.

**`DBG-1`** — The render path threads the correct `run_log_path`, but the mouse hit-test path rebuilds a `ContainerInfoState` with `""` (`crates/jackin-launch/src/tui/subscriptions.rs:173,370,373`). An empty string is still `Some("")`, so the `Diagnostics log` row exists but copies nothing. Pass the same real `run_log_path` (and run id) into the hit-test state builders. Acceptance: launch hover+click on `Run ID` and `Diagnostics log` copy the exact bare run id / JSONL path (non-empty); the version row never contains `.jackin/data/diagnostics`.

**`DBG-2`** — `crates/jackin-tui/src/components/container_info.rs:385` (`render_debug_info_hint`) draws the hint at `dialog_rect.y + height + 1` — a floating row under the dialog. Axis derivation is already correct; **placement** is the bug. Move the hint into each surface's fixed footer row so bottom chrome stays `status → separator → hint`, with no floating line under the box. Shared-component change — apply once so console, launch, capsule all match. Update `dialogs.mdx`/`chrome.mdx` to state footer-only (coordinate with `PRE-1`). Layout test: hint in footer, one separator above status bar, no hint text immediately below the dialog rect.

**`DBG-3`** *(needs smoke)* — Capsule hover/click geometry (`crates/jackin-capsule/src/daemon/mouse_input.rs:46`, `…/input_dispatch.rs:600`, `container_info.rs:499`, `dialog_layout.rs:401`) shares the same rect across render and hit-test; tests pass; the audit's off-by-one may predate the Run-ID-first ordering fix. Do not refactor on suspicion. Run the live capsule dialog with `--debug`, hover/click each row, read the diagnostics run JSONL. Close `done` with that evidence, or, if it still mis-targets, make every capsule call site pass the identical rendered rect and add a coordinate test (incl. the horizontally-scrolled case).

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

**`DLG-3`** — `settings/view.rs:477-478` (Kind row, 1-cell cursor) vs `:511,540` (source rows, 2-cell); no shared selectable-row. Use the `PRE-3` finding. Route `render_auth_source_line` / `render_auth_source_folder_line` and the Kind row through the same two-cell cursor-gutter helper; fix both Settings and workspace-editor Auth (or unify them). Acceptance: selected shows `▸`, unselected `  `; label start column identical for `Mode`, `Source`/`Source folder`, `+ Override for a role`.

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
