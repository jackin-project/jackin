# PR #495 Fix Plan

- **PR:** https://github.com/jackin-project/jackin/pull/495
- **Branch:** `feature/tui-architecture`
- **Source audit:** `PR-495-REVIEW.md`
- **Purpose:** one working file for the fixes that must land in this PR before it is ready to merge.

This file is the collection point for operator notes, architectural decision details, TUI decision details, and the concrete fixes that follow from them. Keep the evidence-heavy review notes in `PR-495-REVIEW.md`; keep this file focused on decisions, required changes, and verification.

## How To Use This File As A Goal

This file is meant to be handed to Codex as the implementation objective. Treat it as the working spec for finishing PR #495, not as a brainstorming document.

When following this file:

1. Start by reading the source-of-truth TUI references listed below.
2. Work from the **Definition Of Done** and **Fix Queue** before choosing implementation order.
3. Prefer shared primitives and component consolidation over local fixes. A one-line local patch is not acceptable if it leaves a parallel TUI behavior in place.
4. Keep this file current while working: mark rows done only after code, tests, docs, and lookbook/snapshot coverage agree.
5. If code and docs disagree, do not guess silently. Update the docs and implementation in the same PR so the published docs remain the spec.
6. If a listed item cannot reasonably land in this PR, move it to an explicit roadmap follow-up with a concrete reason and leave this file saying exactly what was deferred.

### Goal Runner Rules

Use these rules while executing the goal:

- **Status values:** use `pending`, `in_progress`, `done`, or `deferred` in tables. Do not invent new states.
- **One active phase at a time:** keep the phase currently being implemented as `in_progress` in this file or in the goal status update. Finish or explicitly defer it before moving to a non-dependent phase.
- **Done means verified:** mark a row `done` only after code is changed, tests/snapshots/docs are updated where needed, and the relevant verification command has passed or a failure is documented with cause.
- **Deferred means tracked:** mark a row `deferred` only when there is a roadmap item or follow-up section naming the exact remaining files/behaviors. "Too large" is not enough.
- **No silent scope shrink:** if investigation proves an item is not actually required, explain why and link the evidence in this file before removing or downgrading it.
- **No screenshot-only fixes:** every visual fix needs a code-level cause, a shared-component decision, and a regression test/snapshot where practical.
- **No local styling forks:** if a local fix adds a second color, border, hint, row, scroll, or click style, stop and extract or extend a shared helper instead.
- **Keep the operator-note table synchronized:** when a detailed section is completed, update the matching summary table row and checklist item in the same change.

### Completion Report Required

When the goal run finishes, the final report should include:

- checklist phases completed;
- table rows still `pending` or `deferred`, with exact reason;
- shared components/helpers changed or added;
- docs and lookbook artifacts updated;
- verification commands run and their result;
- known residual risk, especially around capsule scrollback, Debug info geometry, and roadmap deferrals.

### Source-Of-Truth References

Before editing TUI code, read these references and use them as acceptance criteria:

| Reference | What it governs |
|---|---|
| `docs/content/docs/reference/tui/components.mdx` | Component reuse hard rule, component homes, picker/list renderer ownership, settings/workspace parity |
| `docs/content/docs/reference/tui/architecture.mdx` | Elm Architecture boundaries, source-code locations, typed effects, Ratatui render purity |
| `docs/content/docs/reference/tui/dialogs.mdx` | Modal sizing, five-slot dialog padding, Debug info contract, footer-only hints, modal click lifecycle |
| `docs/content/docs/reference/tui/chrome.mdx` | Bottom chrome order, status bar/chip behavior, focus-visible borders, shared Debug info across surfaces |
| `docs/content/docs/reference/tui/navigation.mdx` | W3C keyboard roles, hover/click affordances, cursor gutter, focusability, scroll hint/scrollbar coupling |
| `docs/content/docs/reference/tui/visual-design.mdx` | PHOSPHOR palette, default-background fills, panel title spacing, body inset, copyable value styling |
| `docs/content/docs/reference/adrs/adr-003-ratatui.mdx` | Ratatui as the accepted rendering library and testable buffer model |
| `docs/content/docs/reference/adrs/adr-004-pane-body-rendering.mdx` | Capsule pane body rendering through jackin-term + Ratatui, and the allowed custom body-widget boundary |
| `crates/AGENTS.md` | Rust 2024 module layout, workspace lint inheritance, clippy/suppression discipline |
| `crates/jackin-tui/COMPONENTS.md` | Current component inventory, owner modules, lookbook coverage, maturity |
| `docs/content/docs/reference/tui/lookbook/*.mdx` plus `docs/public/tui-lookbook/*.svg` | Visual regression references for shared component output |

### Non-Negotiable TUI Invariants

These are the design rules this plan is trying to restore. Every fix below should be judged against them:

- **One meaning, one shape.** If two screens show the same concept, they use the same component or shared primitive. Debug info, error dialogs, scrollable panels, hint bars, selectable rows, and copyable values must not have per-surface look/behavior forks.
- **Reusable first, local only once.** Before adding TUI code, search `crates/jackin-tui`, the owning surface's `src/tui`, and transitional root-console adapters. If a pattern already exists, extend that component/helper. If a new pattern appears in more than one surface, move it to `jackin-tui` before the PR lands.
- **Ratatui is the TUI rendering path.** Compose UI through Ratatui widgets, `Frame`, `Buffer`, `Rect`, `Layout`, and shared `jackin-tui` components. Raw ANSI is limited to documented tail/backend responsibilities and must not become a second chrome/layout implementation.
- **Elm Architecture boundaries hold.** Model owns visible state, update handles deterministic state transitions, view is pure rendering, and external work travels through typed effects. Components do not execute Docker/git/filesystem/network work.
- **Footer-only hints.** Hints live in the fixed bottom hint row for the active focus context. Dialogs and overlays do not draw private floating hint rows inside or below their boxes.
- **Hints are true affordances.** A scroll hint appears only when the matching scrollbar is visible and that axis can actually move. A copy hint appears only when a copy target can actually be hovered/clicked/copied.
- **Focus is visible and singular.** Exactly one active interaction container per visible layer has the bright `PHOSPHOR_GREEN` focus cue. Background panels and parent dialogs behind a child dialog do not keep a competing green border or visible `▸` cursor.
- **`▸` is focus-gated but the gutter is stable.** Hiding the cursor glyph must never move row text. Selectable rows reserve the same two-cell cursor gutter whether focused, unfocused, selected, or behind a modal.
- **Modal backdrops occlude body content.** When a modal owns the content area, noisy body content behind it is cleared to the default background. Reserved bottom chrome/status remains visible and is rendered by its normal owner.
- **Dialog padding is symmetric.** Content-plus-action dialogs use the five-slot inner layout: leading spacer, content, spacer, action row, trailing spacer.
- **Scrollable geometry has one source.** Renderer, input, hit-testing, scrollbars, hover overlays, copy targets, and hints derive from the same content width/height, viewport rect, and clamped scroll offsets.
- **Scrollable code is shared code.** Do not reimplement viewport math, thumb math, offset clamping, cursor-follow slicing, wheel routing, or scrollbar hit areas. Use `jackin_tui::scroll`, `scrollable_panel`, `dialog_layout`, `DialogBodyScroll`, `ScrollAxes`, and related helpers, or extract a new reusable helper when the existing one is too high-level.
- **Clickable targets look clickable.** Resting style is distinct, hover changes color/style, and terminals that support it switch to the hand pointer. Click hit-test geometry must match the rendered target.
- **Default background, named colors only.** Backdrops/surfaces use terminal default background tokens, not forced black. New color use goes through named theme tokens, never inline RGB literals.
- **Modern Rust stays idiomatic.** New code follows Rust 2024 self-named module layout, workspace lints, small typed state, explicit enums/messages over stringly dispatch, no `unwrap`/`expect` in runtime input paths, and no broad `allow` suppressions. Prefer well-named helpers and testable pure functions over clever inline calculations.

### Definition Of Done

This PR is not ready until all of these are true:

- Every Debug info entry point in console, launch, and capsule uses the shared Debug info model/renderer for row order, labels, scroll, copy, hover, links, hints, and clipping.
- Every scrollable dialog/body/panel advertises scroll only for axes that overflow and uses shared scroll state/geometry or a documented shared lower-level primitive.
- Capsule pane chrome, focus border, scrollbar styling, and footer hints match the shared scrollable panel look represented by the Global mounts block and lookbook `scrollable-panel/mounts`.
- Dialogs with actions use the canonical five-slot padding in code, tests, and lookbook stories.
- Selectable row text columns do not shift when focus/cursor visibility changes.
- All new/changed click targets have hover styling, pointer-shape routing where supported, click-to-action, and tests for hit geometry.
- TUI docs are updated wherever the intended rule differs from the current published text.
- Lookbook stories or snapshots are updated for every shared component whose visual output changes.
- `cargo fmt --check`, Clippy, workspace tests, docs build/link/type/test gates, and PR checks pass.

## Goal Checklist

Use this checklist as the operational task list. Check an item only when the implementation, tests, docs, and this file agree.

### Phase 0 — Orient And Protect The Worktree

- [ ] Confirm the active branch is `feature/tui-architecture` and PR #495 is the active PR.
- [ ] Review `git status --short` and preserve unrelated operator changes.
- [ ] Read `PR-495-REVIEW.md` for evidence-heavy audit details before deleting or moving code.
- [ ] Read every source-of-truth TUI reference listed above before editing TUI code.
- [ ] Search for existing helpers/components before adding any TUI code: `jackin-tui`, the owning surface's `src/tui`, and transitional root-console adapters.

### Phase 1 — Settle Specs And Docs First

- [ ] Resolve every item in **Spec Gaps To Resolve While Implementing**.
- [ ] Decide the canonical Debug info version label and update code/docs/tests/lookbook consistently.
- [ ] Update docs where operator decisions supersede current docs, especially Debug info backdrop behavior and build-log click dismissal.
- [ ] Ensure roadmap status matches actual evidence: done, partial, deferred, or follow-up.
- [ ] Keep published docs free of stale PR-state claims and ensure references name current behavior, not intended future behavior.

### Phase 2 — Architecture Cleanup

- [ ] Delete orphaned migrated source files identified in `PR-495-REVIEW.md`.
- [ ] Verify extracted crates own their relevant tests where practical.
- [ ] Remove `jackin-diagnostics -> jackin-tui` by moving non-visual helpers lower in the stack.
- [ ] Hoist duplicated per-crate lint policy into `[workspace.lints]` and opt crates in with `lints.workspace = true`.
- [ ] Reconcile documented closed-enum dispatch counts with actual code or update the docs.
- [ ] Run targeted checks after each structural cleanup so dead-path edits are caught early.

### Phase 3 — Shared Debug Info

- [ ] Audit every Debug info entry point in console, launch, and capsule.
- [ ] Route every entry point through `DebugInfo` / `ContainerInfoState` / `render_container_info`.
- [ ] Remove parallel Debug info renderers, row builders, copy behavior, hover behavior, hint generation, or scroll handling.
- [ ] Fix launch data wiring so `jackin version` cannot receive the diagnostics JSONL path.
- [ ] Persist and clamp shared `DialogBodyScroll` state for surfaces that rebuild dialog state each frame.
- [ ] Make horizontal/vertical scrolling, clipping, footer hints, hover, copy, copied feedback, and hyperlink overlays share the same geometry.
- [ ] Ensure `Run ID` and `Diagnostics log` are copyable everywhere they show copy affordances.
- [ ] Ensure the Debug info backdrop clears noisy content with the default background while preserving reserved bottom chrome/status.
- [ ] Add cross-surface tests for row order, row labels, copy payloads, scroll axes, clipping, hover/click hit-testing, and backdrop.

### Phase 4 — Scroll Architecture And Reuse

- [ ] Audit every scrollable component, dialog, overlay, pane, panel, list, and footer hint producer.
- [ ] Replace static scroll hints with `ScrollAxes` / `scroll_hint_spans` or equivalent shared overflow-derived state.
- [ ] Ensure no scrollbar appears when content fits and no scroll hint appears when the matching scrollbar is absent.
- [ ] Ensure renderer, input, hit-testing, drag, hover/copy overlays, resize clamps, and hints consume the same content extents and rect.
- [ ] Replace bespoke viewport, thumb, offset, and wheel math with `jackin_tui::scroll`, `scrollable_panel`, and `dialog_layout`.
- [ ] If capsule pane PTY cells cannot use `render_scrollable_block` directly, extract a reusable scrollable panel shell into `jackin-tui`.
- [ ] Add fit-content, horizontal-only, vertical-only, both-axes, resize, and max-scroll tests.
- [ ] Add debug telemetry before behavioral changes if current logs cannot prove the scroll/render state consumed in the same frame.

### Phase 5 — Capsule Pane Chrome And Scrollback

- [ ] Preserve the PTY streaming body unless shared chrome/scroll correctness requires a body change.
- [ ] Replace capsule-specific pane border/focus palette with shared `Panel`/scrollable-panel green active/inactive behavior.
- [ ] Make pane title styling, body inset, border focus, scrollbar track/thumb, and focus transfer match the Global mounts reference.
- [ ] Show pane scrollbars only on actual overflow.
- [ ] Make pane vertical scrollback monotonic and stable for wheel/touchpad bursts.
- [ ] Keep visible slice, scrollback offset, scrollbar thumb, cursor visibility, and footer hints derived from the same state.
- [ ] Verify alternate-screen panes do not flicker between live tail and retained scrollback while the operator is browsing history.
- [ ] Add capsule render/input tests for long lines, repeated prompts with no overflow, scrollback, resize, and split panes.

### Phase 6 — Dialogs, Rows, And Click Targets

- [ ] Render `Git repository detected` with the canonical five-slot dialog layout.
- [ ] Fix file-browser parent gutter so hiding `▸` behind a child dialog does not shift row text.
- [ ] Fix Auth source/source-folder rows so every selectable row reserves the cursor gutter consistently.
- [ ] Make every `+ ...` creation sentinel use the same action-row color, weight, selected effect, and cursor-gutter behavior, including `+ New workspace` and `+ Add mount`.
- [ ] Fix `ErrorDialog` spacing in the shared component, not at one caller.
- [ ] Update lookbook stories/SVGs for any changed shared dialog or panel output.
- [ ] Ensure inside clicks on the build-log overlay are swallowed unless they hit a real target; close only with `Esc`/`q`.
- [ ] Ensure all clickable targets have distinct resting style, hover lift, pointer-shape routing where supported, and click-to-action tests.

### Phase 7 — Verification And Closeout

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- [ ] Run `cargo nextest run --workspace --all-features`.
- [ ] Run docs checks from `docs/`: `bun run build`, `bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test`.
- [ ] Regenerate/check lookbook SVGs when shared component visuals change.
- [ ] Run or inspect `gh pr checks 495`.
- [ ] Update this checklist statuses and the operator-notes table so remaining work is explicit.
- [ ] If anything is deferred, create/update roadmap follow-up and state the exact files/behaviors left.
- [ ] Produce the completion report described in **Completion Report Required**.

### Spec Gaps To Resolve While Implementing

These are not optional open-ended questions. They are places where the current documentation, code, or screenshots do not fully agree, so the implementation must settle the contract and update the references.

| Gap | Required resolution |
|---|---|
| Debug info backdrop wording differs across docs and operator expectation | Implement the stricter operator-visible rule: modal body/background content is hidden by a default-background backdrop; reserved bottom chrome/status remains visible. Update `dialogs.mdx`, `chrome.mdx`, and any lookbook/story copy to say the same thing. |
| Debug info version row naming is inconsistent (`jackin version` vs `jackin`) | Keep one canonical label across `DebugInfo::into_state()`, tests, docs, and lookbook. The current operator-facing screenshots use `jackin version`; if changing to `jackin`, update every reference and test intentionally. Do not leave mixed labels. |
| Build-log overlay docs say click can dismiss, but desired behavior is keyboard close only | Update `chrome.mdx` so `Esc`/`q` close the overlay; inside body clicks are swallowed unless they hit a real interactive target such as a scrollbar. |
| Capsule pane chrome currently has a capsule-specific palette | Replace or wrap it with the shared panel/focus/scrollbar palette used by the Global mounts block. If a lower-level shell is needed for PTY cells, define that shell as a reusable `jackin-tui` primitive and document it. |
| Scroll hint producers are scattered | Collapse each producer onto `ScrollAxes` / `scroll_hint_spans` or the equivalent shared panel overflow state. Any remaining static scroll hint must be justified by visible overflow in the same render path. |
| Settings and workspace editor Auth rows may be separate render paths | Audit both. A fix in Settings Auth that leaves workspace editor Auth drifting, or the reverse, violates the settings/editor parity rule. |

## Operator Notes To Collect

Add each new item here as it comes in.

| Status | Area | Summary | Decision / Constraint | Fix |
|---|---|---|---|---|
| pending | TUI / Debug info | Every debug-info display must use the same component and behavior on every screen | Screens may provide more or fewer facts, but the shared component owns ordering, labels, copy affordances, scrolling, hints, and rendering | Audit all Debug info call sites and route them through the shared `DebugInfo` / `ContainerInfoState` / `render_container_info` path |
| pending | TUI / Debug info | Launch Debug info displays the `jackin version` row with the diagnostics JSONL path as its value | Display facts only when they are known; never fill unknown fields with unrelated placeholders or adjacent values | Fix data wiring so `jackin_version` receives the host jackin' version and `diagnostics_log_path` receives only the JSONL path |
| pending | TUI / Debug info | Horizontal scroll can render clipped long values outside the dialog body | Scrolled content must stay clipped to the dialog inner area; borders, footer hints, and adjacent chrome must never be overpainted | Fix shared renderer clipping/viewport math and add snapshot or buffer assertions for right-scroll at max offset |
| pending | TUI / Debug info | `Run ID` and `Diagnostics log` show copy affordances but are not copyable/interactable on the launch screen | Copy affordance must mean the value is hoverable, clickable, copies to clipboard, and gives copied feedback | Wire hover hit-testing, pointer shape, click-to-copy, clipboard effect, and copied-row feedback through the shared component behavior |
| pending | TUI / Debug info | Capsule/launch Debug info can show animated/background content behind the dialog | Debug info must paint an opaque default-background backdrop over everything except reserved bottom status chrome | Clear/hide the content area behind the dialog on every surface before rendering the shared component |
| pending | TUI / Hints footer | Debug info hints can render as a floating row under the dialog instead of in the fixed footer chrome | Modal hints always live in the reserved footer/hint rows, above the separator and status bar; never under the dialog body | Route Debug info hints through each surface's standard footer renderer and update docs where current wording is ambiguous |
| pending | TUI / Hints footer | Scroll hints can be shown when no horizontal or vertical scrolling is possible | A scroll hint is allowed only for axes whose scrollbar is visible; no scrollbar means no scroll hint for that axis anywhere in jackin' | Audit all footer/hint producers and derive scroll hint axes from the same overflow gate that draws the scrollbar |
| pending | TUI / Debug info | Capsule Debug info horizontal scroll does not work or is not advertised consistently | Debug info scroll behavior and hints must be identical across console, launch, and capsule | Persist and clamp the shared scroll state in capsule and route wheel/keyboard through the same shared scroll-axis logic |
| pending | TUI / Debug info | Capsule Debug info hover/click targets are misaligned; hovering/clicking `Run ID` targets `Container ID` | Visual row placement, hover state, click hit-test, copied feedback, and OSC/link overlay must all use the same geometry | Fix capsule rect/row-offset math so `container_info_copy_payload_at` receives the exact rect used to render the dialog |
| pending | TUI / Reuse | Capsule Debug info shows a horizontal scrollbar but horizontal scroll input does not move it | Scrollbar rendering, scroll state, keys, wheel, hints, and hit-testing must all use the shared dialog-scroll helpers | Remove any bespoke capsule scroll path and use `DialogBodyScroll`, `dialog_scroll_axes`, and `render_scrollable_dialog_body` end-to-end |
| pending | TUI / Capsule panes | Capsule pane horizontal scrollbar renders/updates inconsistently as pane content and terminal size change | Pane scrollbars must use shared reusable geometry and clamp/update on every render/resize/content change | Audit pane scrollbar rendering/input against shared horizontal-scroll helpers and replace bespoke drift-prone math |
| pending | TUI / Capsule panes | Capsule pane scrollback scrollbar position does not match the visible scrolled content | Scroll offset, viewport height, content height, and scrollbar thumb position must derive from one shared state model | Fix pane scrollback offset/thumb geometry so scrolling down lands at the correct visible content position |
| pending | TUI / Capsule panes | Capsule pane vertical scroll moves unpredictably with mouse/touchpad input, including flicker and apparent direction reversals | Pane scroll must follow the operator's wheel/touchpad direction monotonically and feel like every other vertical scroll surface in jackin' | Normalize wheel bursts through the shared scroll model, coalesce/clamp per frame, and stop live PTY redraws from fighting the operator's retained scrollback view |
| pending | TUI / Capsule panes | Capsule pane shows a right-border scrollbar even when the visible shell content fits and there is nothing to scroll | Scrollbars are overflow affordances only; no overflow means no thumb, no scroll hint, and no wheel-scroll behavior | Fix the pane content/scrollback length gate so prompt redraws or stale scrollback counts do not create a false scrollbar |
| pending | TUI / Capsule panes | Capsule pane chrome and scrollbar do not match the Global mounts scrollable block look and feel | Capsule panes must use the same title color, active/inactive border colors, click-to-focus effect, focus removal effect, scrollbar style, and footer hints as the Global mounts block | Reuse `Panel` / `render_scrollable_block` chrome primitives around the terminal body, keeping only the internal terminal-cell body custom if necessary |
| pending | TUI / Build log overlay | Docker build-log overlay closes when clicked inside | Inside clicks with no target must be swallowed; build log closes only on `Esc`/`q` | Stop mapping ordinary overlay clicks to `BuildLogClosed`; only scrollbar clicks/drags affect scroll |
| pending | TUI / Dialog layout | `Git repository detected` prompt has wrong top padding | Dialogs with content plus buttons must use the canonical five-slot symmetric inner layout | Render the git prompt with `dialog_inner_chunks`, matching `git_prompt_rect` height and preserving one leading spacer row |
| pending | TUI / File browser | Opening the `Git repository detected` child dialog removes the parent file-browser list gutter | Item text columns must remain stable whether the selection cursor glyph is visible or suppressed | Keep the reserved cursor/gutter width while hiding or dimming the parent selection behind the child dialog |
| pending | TUI / Auth editor | Auth source/source-folder rows do not reserve cursor space consistently | Selectable auth rows must always reserve cursor space; selected rows show `▸`, unselected rows keep the same gutter width | Route source/source-folder rows through the same selectable-row renderer or cursor-gutter helper as the other Auth rows |
| pending | TUI / Action rows | `+ New workspace` does not match the `+ Add mount` action-row color/effect | Every `+ ...` creation sentinel uses one shared action-row style and selected effect across workspaces, editor, settings, and pickers | Route `+ New workspace` through the same `action_row_style(selected)` / shared creation-sentinel helper as `+ Add mount` and audit every other add/new row |
| pending | TUI / Error dialog | `Load role failed` shows two blank rows between message text and the `OK` button | Single-action dialogs must have exactly one blank spacer row between content and action | Fix the shared `ErrorDialog` body sizing so all surfaces and the lookbook use the same one-row content-to-button spacing |

## Architectural Decisions To Preserve

Use this section for the reasoning behind architecture-level fixes, especially where the PR changes crate boundaries, workspace lint policy, diagnostics ownership, runtime boundaries, or roadmap status.

### Workspace Split Is Directionally Correct, But Incomplete

The PR's broad architecture direction is sound: split the old monolith into tiered crates and move shared terminal/TUI concepts into reusable libraries. The current branch still carries incomplete migration residue:

- `crates/jackin/src/runtime/`, `crates/jackin/src/isolation/`, and related old source trees contain about 32k LOC of orphaned, committed code.
- The shim files re-export code from the extracted crates, so those shadowed child files are not loaded by Rust.
- This creates drift risk because future fixes can land in the dead copy rather than the live extracted crate.

**Decision:** finish the extraction by deleting the orphaned source trees instead of keeping copied legacy code beside the new crates.

**Fix:** delete the 67 orphaned files identified in `PR-495-REVIEW.md`, then verify the live crate graph still builds.

### Extracted Tests Should Follow Extracted Code

Some tests for extracted modules still live in the consumer `jackin` crate and exercise re-export shims instead of the owning crate.

**Decision:** crate ownership should be test ownership. A crate-level test command such as `cargo test -p jackin-config` should meaningfully validate the crate in isolation.

**Fix:** move extracted-module tests into their owning crates where practical.

### Diagnostics Must Not Depend On TUI

`jackin-diagnostics` currently depends on `jackin-tui` for small helpers such as ANSI stripping, output pruning, and home-path shortening.

**Decision:** diagnostics and telemetry are lower-level than UI. Shared helpers should live in `jackin-core` or a small utility crate, not force diagnostics to depend on a TUI crate.

**Fix:** relocate the shared helpers and remove the `jackin-tui` dependency from `jackin-diagnostics`.

### Lint Policy Should Be Single-Source

The documented lint guarantee says `clippy::mod_module_files = "deny"` is workspace-enforced, but the manifests currently duplicate drifting lint tables per crate.

**Decision:** lint policy is workspace architecture, not per-crate copy-paste. Use one `[workspace.lints]` table and opt crates into it with `lints.workspace = true`.

**Fix:** hoist the lint policy to the root manifest, apply it consistently to all crates, and update `crates/AGENTS.md` so the documentation matches the actual enforcement.

### Roadmap Status Must Match Evidence

Several roadmap checklist items are marked done while their notes say deferred, unmeasured, or hardware-smoke pending.

**Decision:** roadmap status should reflect verified state, not intended state. Code-complete but unverified work should be marked partially complete or tracked as follow-up.

**Fix:** update roadmap pages and the roadmap index for any item this PR ships, advances, defers, or invalidates.

## TUI Decisions To Preserve

Use this section for decisions about Ratatui primitives, capsule rendering, terminal model ownership, interaction state, and visual/behavioral parity.

### Scrollable Components Must Be Shared End-To-End

Scrolling is one of the most drift-prone areas in jackin' because it touches rendering, input, focus, hints, hit-testing, resize, clipping, and copied/link overlays. The design rule is therefore stronger than "make it look similar": every scrollable behavior must use shared scroll primitives end-to-end, or extract a new reusable primitive before adding a second implementation.

**Decision:** no surface owns custom scroll math unless it is the first and only caller of a genuinely new behavior. The moment console, launch, capsule, dialogs, or lookbook need the same behavior, the shared implementation belongs in `jackin-tui`.

**Canonical helpers and responsibilities:**

| Concern | Preferred owner |
|---|---|
| Offset clamping, thumb metrics, cursor-follow math, drag/track geometry | `jackin_tui::scroll` |
| Bordered passive scroll blocks, panel border focus, horizontal/vertical scrollbars | `jackin_tui::components::scrollable_panel` |
| Scrollable dialog bodies, per-axis overflow, scroll hints | `jackin_tui::components::dialog_layout` (`DialogBodyScroll`, `dialog_scroll_axes`, `scroll_hint_spans`, `render_scrollable_dialog_body`) |
| Debug info value rows, copy/hyperlink geometry, long structured values | `jackin_tui::components::container_info` |
| Selectable rows, `▸` gutter, full-width highlight, selection-follow viewport | `jackin_tui::components::select_list` or a shared row helper with equivalent behavior |
| Panel chrome, body inset, title padding, focus border | `jackin_tui::components::panel` |

**Implementation rule:** the same content lines/snapshot and the same rendered rect feed all of these:

- content width/height measurement;
- scroll offset clamp;
- visible slice;
- scrollbar visibility;
- scrollbar thumb length/position;
- keyboard and wheel input;
- drag/hit-test areas;
- hover/click target coordinates;
- footer hint axes.

If rendering uses one width and input uses another, or if hit-testing uses a rect different from rendering, the implementation is wrong even if the screenshot happens to look right.

**Capsule-specific boundary:** capsule pane bodies may keep a custom terminal-cell body renderer because ADR-004 accepts `PaneBodyWidget` over jackin-term snapshots. The surrounding scroll shell, panel chrome, focus behavior, scrollbar metrics, overflow gates, and hint decisions should still come from shared helpers or newly extracted lower-level shared helpers. A custom PTY body is not permission to hand-roll pane chrome or scrollbars.

**Verification:** every scroll fix should include at least one test that proves a shared gate is used consistently: no overflow means no scrollbar and no hint; overflow means scrollbar and matching hint; scrolling changes the visible slice and thumb position together; resize clamps the offset and recomputes the thumb. Cross-surface bugs require cross-surface coverage.

### Shared TUI Primitives Should Have One Adoption Path

The PR introduces reusable primitives such as `HoverTracker`, `FocusOwner`, shared scroll helpers, and click classification. Some surfaces still keep scattered local state or parallel behavior.

**Decision:** shared primitives should replace the old duplicated state paths, not coexist with them indefinitely.

**Fix:** migrate the remaining surfaces onto the shared primitives, or explicitly record each deferred migration as roadmap follow-up instead of marking it done.

### Capsule ANSI Rendering Is A Parallel TUI Implementation

The capsule still renders some chrome directly through ANSI/VT100 output rather than through the shared Ratatui-based primitives.

**Decision:** this is the largest remaining TUI duplication risk. If capsule chrome cannot migrate in this PR, it needs an explicit roadmap item with the reason and remaining scope.

**Fix:** either migrate capsule chrome onto the shared TUI primitives or open/update a roadmap item for capsule ANSI-to-Ratatui migration.

### Debug Info Must Be One Shared Component

Every screen that displays debug information must use the same component and interaction model. The only allowed difference between screens is which facts are available:

- If a fact is known, pass it to the shared model and display it.
- If a fact is not known yet, omit the row.
- Do not display placeholder values for unknown debug facts unless the value is genuinely a loading state for that exact fact.
- Do not hand-render a separate Debug info dialog per screen.

The existing shared model already expresses this direction: `DebugInfo` accumulates optional facts and turns them into a `ContainerInfoState`, while `render_container_info` owns the dialog body.

**Decision:** `DebugInfo` / `ContainerInfoState` / `render_container_info` is the canonical Debug info path across console, launch, and capsule.

**Fix:** audit every `Debug info` / container-info call site and remove any parallel rendering or locally invented behavior. Keep screen-specific code limited to gathering known facts and preserving local open/copy/hover/scroll state.

### Launch Debug Info Version Value Is Wrong

The launch Debug info screen currently shows:

```text
jackin version  : /Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/diagnostics/runs/jk-run-357ae4.jsonl
Diagnostics log : /Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/diagnostics/runs/jk-run-357ae4.jsonl
```

The `jackin version` value is completely wrong. It is being populated with the diagnostics log path on this screen.

**Decision:** each row must display only its own fact. `jackin version` displays the host jackin' version string. `Diagnostics log` displays the diagnostics JSONL path. If the version is unavailable, omit the version row.

**Fix:** trace the live launch path that builds `DebugInfo` and make sure the `jackin_version` argument is always the host version, never `run_log_path`. Add a regression test that asserts the version row does not contain `.jackin/data/diagnostics` and the diagnostics row does.

### Debug Info Horizontal Scroll Must Clip To The Dialog

When the launch Debug info dialog is scrolled all the way right, long path values leak outside the intended body area and appear to overpaint or escape the bordered dialog. The observed right-scrolled state shows partial value text at the left edge and an extra trailing fragment such as `jsonlnli` inside the dialog line.

**Decision:** horizontal scrolling is allowed, but the rendered viewport must be clipped to the dialog inner area. No row text, hyperlink overlay, copy affordance, or copied feedback may render outside the body rectangle.

**Fix:** fix the shared `render_container_info` / scrollable-dialog-body clipping and the matching hyperlink/copy placement math. Add a regression test that renders a long diagnostics path at max horizontal scroll and asserts all non-border content stays inside the dialog inner rect.

### Debug Info Copy / Hover / Pointer Contract Must Work Everywhere

The project already documents the canonical Debug info behavior in `docs/content/docs/reference/tui/dialogs.mdx`:

- `Run ID` is copyable.
- `Container ID` is copyable.
- `Diagnostics log` is copyable and an OSC 8 `file://` hyperlink.
- Enter copies the first copyable row in canonical order, so it copies `Run ID` whenever that row exists.
- Mouse click copies the copyable value under the pointer.
- Hover feedback applies only to copyable value cells and their copy affordance.
- The dialog stays open after copy so copied-row feedback can render.

The launch screen currently violates that contract: `Run ID` and `Diagnostics log` display the copy icon / copy hint, but the values are not actually hoverable or copyable.

**Decision:** visible copy affordance is a contract, not decoration. If a row displays the copy icon or appears as a hyperlink, it must support hover, pointer/cursor affordance, click-to-copy, clipboard write, and copied feedback. Following the W3C interactive-control expectation, hovering a clickable value should visibly change color and the terminal pointer shape should change to pointer when supported.

**Fix:** wire the launch Debug info surface to the same copy and hover behavior as console/capsule:

- Use shared `container_info_copy_payload_at` hit-testing for both hover and click.
- Update the persisted `container_info_hover` row on mouse movement.
- Set pointer shape to pointer when the mouse is over a copyable value/copy icon/hyperlink target.
- On click, copy the row payload to the clipboard through the existing terminal clipboard path.
- Mark the copied row so the shared renderer displays copied feedback without closing the dialog.
- Preserve horizontal-scroll awareness so hover/click targets follow the scrolled value positions.

**Verification:** add launch tests that place the mouse over `Run ID` and `Diagnostics log`, assert hover state changes, assert the pointer shape/clickable classification, click each row, and assert the copied payload is exactly the bare run id or diagnostics JSONL path.

### Debug Info Backdrop Must Hide Background Content

The Debug info dialog can currently appear over live launch/capsule content without an opaque backdrop. In the observed launch screen, digital-rain characters remain visible around and visually behind the dialog, making the modal look transparent and noisy.

**Decision:** every Debug info dialog must use a plain default-background backdrop and hide everything behind it except the reserved bottom status chrome. The dialog should feel like it owns the screen region, not like a panel floating above active content. The status bar remains visible because it is persistent chrome; the animated/body content behind the dialog does not.

**Fix:** before rendering Debug info on every surface, clear the content area behind it to the default dialog/background color. Apply this consistently for console, launch, and capsule. The backdrop should cover the modal content area while preserving the bottom chrome stack reserved for hints/separator/status.

**Documentation update needed:** the current references are not fully aligned:

- `docs/content/docs/reference/tui/dialogs.mdx` says Debug info is status-preserving and persistent top chrome may remain visible.
- `docs/content/docs/reference/tui/chrome.mdx` says Debug info must not erase persistent chrome and paints only panel/backdrop inside the content area.

Update these references to the stricter rule: Debug info hides all background/body content with the default backdrop, while preserving only the reserved bottom chrome/status area.

**Verification:** add a render test for launch/capsule Debug info over noisy background content and assert cells outside the dialog but inside the content area are default background/blank, while the bottom status bar remains visible.

### Debug Info Hints Must Use The Fixed Footer

The observed Debug info screen renders hints as a floating line directly under the dialog:

```text
←→ scroll   ↵ copy value   Esc dismiss
```

That is wrong for jackin' modal hints. The TUI reference already defines the rule in `docs/content/docs/reference/tui/dialogs.mdx` and `docs/content/docs/reference/tui/chrome.mdx`:

- A modal's keys replace the active screen keys in the same reserved footer rows.
- There is no floating hint bar under a dialog.
- The bottom chrome row order, from bottom upward, is status/context bar, blank separator, then hint bar.
- Backdrops never cover the footer.
- Launch overlays that preserve the status footer use overlay body, hint row, blank separator row, then white status footer.

**Decision:** Debug info hints must be rendered only through the standard footer/hint bar location for that surface. The dialog renderer may provide `debug_info_hint_spans`, but it should not place a floating hint row under the dialog body.

**Fix:** audit `render_debug_info_hint` and its call sites. Either remove the floating hint renderer or constrain it to write into the surface's fixed footer row. On launch/capsule, use the same bottom chrome stack as every other modal: dialog/backdrop content area, then footer hint row, separator, and status bar.

**Verification:** add layout tests that open Debug info and assert the hint text is in the reserved footer row, with one separator row between hints and the status bar, and no hint text appears immediately below the dialog rectangle.

### Capsule Debug Info Scroll Must Match The Shared Dialog

In the capsule Debug info dialog, long structured values such as `Target` and `Diagnostics log` do not horizontally scroll correctly, and the footer does not advertise horizontal scroll even when values overflow. This violates the one-dialog rule: the capsule may know more facts than launch or console, but the scroll behavior, scrollbars, clipping, and hints must be the same.

In the latest observed state, the horizontal scrollbar is visible on the bottom border, but horizontal scroll input still does not move the content. That narrows the failure: the render path can detect horizontal overflow, but the input/update path is not mutating the same `scroll_x` state that the rendered `ContainerInfoState` uses, or it is clamping/rebuilding it back to zero before the next frame.

**Decision:** capsule Debug info is not a special renderer. It must use the same `DebugInfo` / `ContainerInfoState` / `render_container_info` behavior, with only capsule-specific fact gathering and persisted state around it.

**Fix:** audit capsule `Dialog::ContainerInfo` state and input routing:

- Persist `DialogBodyScroll` on the capsule dialog variant and thread it into the rebuilt `ContainerInfoState`.
- Clamp scroll offsets against the exact rendered dialog rectangle.
- Route keyboard and wheel scroll through the same `DialogBodyScroll` / `dialog_scroll_axes` logic used by the shared component.
- Render footer hints from the same `debug_info_hint_spans` axes calculation, in the fixed footer row.
- Keep horizontal-scroll state active when `Target` or `Diagnostics log` overflow.
- Verify the key bindings for `←`/`→`/`h`/`l` and wheel mappings both update the persisted capsule `DialogBodyScroll.scroll_x`.

**Verification:** add capsule tests that open Debug info with a long target/log path, scroll right, and assert the visible value slice changes, the horizontal scrollbar appears, and the footer includes horizontal scroll hints only when the content actually overflows.

### Horizontal Scroll Helpers Must Be Reused

The reusable rule already exists in `docs/content/docs/reference/tui/components.mdx`: never copy-paste a TUI component; every visual pattern that appears in more than one place must use one shared implementation; extending the shared implementation is required when a new call site needs more behavior.

The dialog-scroll rule also already exists in `docs/content/docs/reference/tui/dialogs.mdx`: overflowing dialog bodies must route through `DialogBodyScroll` plus `render_scrollable_dialog_body`, and each surface must route wheel events to `DialogBodyScroll::on_mouse_scroll`. `ContainerInfoState` is named as the reference implementation for long run IDs and diagnostics paths.

**Decision:** Debug info horizontal scroll must not have a capsule-specific implementation. The entire chain is shared:

- overflow axes: `dialog_scroll_axes`;
- state: `DialogBodyScroll`;
- render: `render_scrollable_dialog_body`;
- hints: `debug_info_hint_spans` / shared footer renderer;
- copy/hyperlink placement: shared `ContainerInfoState` geometry.

**Fix:** find and remove any capsule-local scroll calculation, key mapping, clipping, or hint generation that duplicates the shared helpers. If a helper is missing a capability needed by capsule, extend the helper rather than adding a capsule-only branch.

**Verification:** add cross-surface tests or snapshots for the same long Debug info rows on console, launch, and capsule. The visible slices, scrollbar presence, hint axes, and copy hit-tests should match for the same rect and scroll state.

### Scroll Hints Must Match Visible Scrollability Everywhere

Scroll hints must not be shown unless the corresponding axis can actually scroll. This is global across jackin', not limited to Debug info or capsule panes:

- No vertical overflow means no `↑↓ scroll` hint.
- No horizontal overflow means no `←→ scroll` / `H/L scroll` hint.
- No overflow on either axis means no scroll hint at all.
- If a scrollbar is not visible for an axis, the footer must not advertise that axis.

The docs already state the rule in `docs/content/docs/reference/tui/navigation.mdx`: every scroll hint must reflect the body's real per-axis overflow, and hints must be derived from the same `is_scrollable` gate that draws the scrollbar. This PR should enforce that rule across the implementation, not just for the surfaces that already use `scroll_hint_spans`.

The observed capsule pane screen violates the rule: the footer says `↑↓ scroll` even though the pane shows no meaningful vertical overflow and no scroll affordance should be active.

**Decision:** scroll hints are derived state, never static copy. Every footer/hint producer must receive `ScrollAxes` or equivalent overflow facts from the renderer/layout state that decides scrollbar visibility. If the code cannot prove an axis is scrollable, it must omit that axis from the hint.

**Fix:** audit every scroll hint producer:

- shared `scroll_hint_spans` users;
- console workspace/list footers;
- launch build-log and Debug info footers;
- capsule main view footer (`main_view_hint` / `scrollback_active`);
- capsule dialog/info footers;
- lookbook examples and stories.

Replace static scroll text with axis-aware hints. For capsule main view, do not use a broad `scrollback_active` boolean if it only means "some scroll state exists"; derive visible horizontal/vertical axes from the focused pane's content height/width, viewport size, and current overflow gate.

**Verification:** add tests or snapshots for fit-content cases on each top-level TUI surface: no scrollbar visible and no scroll hint present. Add axis-specific tests for horizontal-only, vertical-only, both-axes, and no-overflow where the hint text exactly matches the visible scrollbars.

### Capsule Pane Horizontal Scrollbar Must Use Shared Geometry

The capsule pane horizontal scrollbar can render and behave incorrectly. In the observed pane screen, the right-side pane scrollbar/thumb and the horizontal overflow behavior feel disconnected from the visible pane content: as more output is added and the terminal/pane size changes, the scrollbar thumb does not update intuitively or consistently.

This is distinct from the Debug info dialog issue. It affects normal capsule pane content, where other horizontal-scroll surfaces in jackin' behave correctly. That points to a likely reuse problem: this pane path may be using bespoke pane scrollbar/viewport math instead of the same shared helpers and clamping rules used by the console panels and shared scrollable components.

The attached screenshot adds a second concrete symptom: after scrolling down in a capsule pane, the scrollbar/thumb does not correspond to the visible content position. The UI says the pane is in scrollback mode, but the visible content is not at the position the thumb implies. This means scrollback offset, content height, viewport height, and thumb placement are drifting apart.

Debug-log evidence from run `jk-run-357ae4` confirms the wheel input is not being lost. At `2026-06-08T11:29:35.184Z` through `2026-06-08T11:29:35.209Z`, repeated wheel events on session `6` move jackin's scrollback state from `before=0 filled=10` to `after=10 moved=true`. Further wheel-down events immediately after that report `before=10 filled=10` and `moved=false`, so the input handler believes the pane is fully scrolled into retained history.

The same log then shows the active PTY feed/render state disagreeing with that scroll position. The feed records `alt_screen=true mouse_enabled=true screen=31x144 cursor=22x53 scrollback=0 scrollback_offset=0`, and subsequent frames keep reporting `scrollback=0 scrollback_offset=0` while sending cursor restores such as `\e[15;15H`. In the screenshot, the visible cursor is therefore the live alternate-screen app cursor in the pane body, around the prompt area under the "Learn more" line, while the wheel handler has already clamped jackin's scrollback offset to the maximum retained value. That mismatch is the bug: the scroll input state, visible content slice, pane thumb, and cursor visibility are not derived from one state model.

The current code shape explains the mismatch risk:

- `input_dispatch.rs` routes wheel events to `session.scroll_by(delta)` when `session.scrollback_counts()` is non-zero.
- `session.scroll_by` updates `session.scrollback_offset` and writes it into `DamageGrid`.
- `compositor.rs` deliberately reports pane scrollbar `filled=0` for alternate-screen apps when building `pane_scrollbars`, suppressing the normal pane scrollbar source even when wheel handling just used retained scrollback.
- `session.feed_pty` reapplies or resets scrollback after every PTY batch, so live alternate-screen redraws can immediately overwrite the operator's scrollback view.
- `append_cursor_state` hides the cursor only when `session.scrollback_offset != 0`, but the post-wheel feed state can return to `0`, making the live app cursor visible again while the UI still appears to be in scrollback mode.

**Decision:** pane scrollbars are not allowed to have one-off geometry. Horizontal overflow must be derived from the same content width, viewport width, scroll offset, and clamp rules used by the renderer. When content changes, panes split/resize, or the terminal resizes, the scrollbar thumb must be recomputed from the new visible geometry in the same frame.

The same applies to vertical scrollback. The visible pane slice and the vertical thumb must be two views of the same state. If the operator scrolls down, the pane must land on the content row represented by the thumb position; if content grows or the pane resizes, the offset must clamp and the thumb must update immediately.

### Capsule Pane Scrollbars Render Only On Overflow

The capsule pane can show a right-border scrollbar even when the visible shell content fits inside the pane and there is nothing meaningful to scroll. In the observed Shell pane, the content is just repeated prompt lines with large empty space below, yet the right border shows a full-height green thumb and the footer advertises `↑↓ scroll`.

**Decision:** a scrollbar is an overflow affordance, not pane decoration. If the pane has no retained scrollback beyond the visible viewport and the rendered content fits, the pane must show no scrollbar, no scroll hint, and wheel input must either be forwarded to the PTY when appropriate or ignored. This matches the shared TUI rule for passive blocks: scrollability is derived from actual content length versus viewport length.

### Capsule Pane Chrome Matches Shared TUI Panels

The capsule pane should look and feel like the **Global mounts** scrollable block in the workspace screen. That block is the concrete reference for:

- title color and title padding;
- active/focused border color;
- inactive/unfocused border color;
- the visual effect when a click or scroll gesture makes the block focusable/focused;
- the visual effect when focus leaves the block;
- the dim dotted scrollbar track;
- the heavy green scrollbar thumb;
- proportional thumb length/position;
- footer hints that advertise scroll only when overflow actually exists.

The current capsule pane chrome reads as a separate implementation. The code currently routes pane borders through `PaneBorderWidget`, which opts into `FocusPalette::CAPSULE_PANE` instead of the shared console `PHOSPHOR_GREEN` focused border. The right-border scrollbar can appear as a solid green column, can show while content does not overflow, and can disagree with the footer hint. That makes the same interaction look like different controls on different screens.

**Decision:** capsule panes should reuse the same shared panel/block chrome and focus machinery as the Global mounts block as far as the architecture allows. Broad refactoring is acceptable here if it moves reusable pieces into `jackin-tui` and makes the shared building blocks intentional. The terminal body itself may remain a custom `PaneBodyWidget` because it renders internal PTY cells, but the surrounding container should not be custom if the shared component can provide it. Border color, focused/unfocused state, title styling, scrollbar glyphs, scrollbar colors, overflow gates, focus transfer, focus removal, and footer hints should come from the same shared primitives used by the Global mounts block.

The capsule PTY/console streaming body is not the target of this visual fix. It is already working and should keep its current behavior. The preferred shape is a reusable jackin-tui panel/shell component that owns chrome, focus styling, scrollbars, and hints, while accepting a body renderer so capsule can stream terminal output inside it without changing the stream renderer's semantics.

This supersedes the current capsule-specific gray focus palette for the pane container. The target visual behavior is the same active and inactive green border look, focus behavior, and scrollbar style used by Global mounts, including split capsule panes.

**Fix:** route capsule pane chrome and scrollbar rendering through shared component decisions:

- render the pane border/title with `Panel` or a shared wrapper using jackin's standard active/inactive green palette instead of hand-rolled capsule gray border styling;
- if `render_scrollable_block` cannot directly render terminal cells, extract a reusable "scrollable panel shell" in `jackin-tui` so capsule can reuse the block chrome and paint/stream `PaneBodyWidget` only inside the body area;
- preserve the existing capsule PTY streaming behavior; fix the look and feel around it unless the stream body must change to support the shared shell safely;
- reuse the same focus-transfer rules as Global mounts: click or wheel over a scrollable pane focuses it, the previous pane/block loses the active border in the same frame, and clicking/focusing elsewhere removes the active border;
- keep non-scrollable passive panes from showing a false focused-scroll state when they have no scroll capability;
- use the same dim dotted track and heavy green active thumb style as the workspace block scrollbar;
- keep inactive/unfocused pane color choices aligned with the shared green inactive panel/block colors;
- compute thumb length and position with `jackin_tui::scroll` helpers, not local formulas;
- make the footer hint use the same overflow gate that decides whether the scrollbar is drawn;
- add render tests that compare capsule pane border color, title styling, thumb position/length, and glyph choices against the shared helper output.

**Fix:** make capsule pane scrollbar visibility use the same content/viewport gate as the renderer:

- compute retained scrollback/content height from the same `DamageGrid` view that renders the pane body;
- treat repeated prompt redraws and cursor movement inside the visible viewport as visible content, not overflow;
- clear or clamp stale `scrollback_offset` and `scrollback_filled` when content shrinks or returns to a non-overflowing state;
- suppress the right-border thumb when `content_height <= viewport_height`;
- suppress `↑↓ scroll` in the footer when no visible pane can scroll.

**Verification:** extend the existing capsule tests that assert no scroll thumb for normal-screen panes without scrollback. Add the concrete shell-prompt case: repeated prompt redraws with blank rows below must render no right-border thumb and no scroll hint.

### Capsule Pane Vertical Scroll Must Be Predictable

The capsule pane vertical scroll can move, but it does not feel like the other vertical scroll surfaces in jackin'. With a mouse wheel or touchpad, a single intended direction can produce flicker and apparent reversal: the pane may move up and down even while the operator is only trying to scroll down. This is especially visible on active alternate-screen panes such as Kimi Code.

This is not acceptable as "scroll works." The expected behavior is directional and monotonic per input burst:

- If the operator scrolls down, the pane should move toward the live tail or stay clamped at the tail; it should not bounce back into older history during that same burst.
- If the operator scrolls up, the pane should move into retained history or stay clamped at the top; it should not jump toward the live tail unless the operator changes direction or the content/state is explicitly reset.
- Touchpad wheel bursts should be coalesced or normalized so rapid small deltas do not cause visible back-and-forth frame fights.
- Active PTY redraws must not fight the retained scrollback view. If jackin' lets the operator browse scrollback for an alternate-screen pane, rendering, cursor visibility, and the scrollbar must all respect that state until the operator returns to the live tail or the scrollback becomes invalid.

**Decision:** capsule pane scrollback is a first-class scroll surface, not a best-effort PTY side effect. It must follow the same interaction quality contract as shared Ratatui scrollable blocks: one input direction produces one visible direction, clamping is stable, and the renderer never flickers between two competing offsets.

**Fix:** normalize the capsule pane wheel path before mutating the view:

- decode SGR wheel buttons into a typed direction/axis before applying any offset;
- ignore horizontal wheel events for vertical scrollback unless a real horizontal pane scroll path exists;
- coalesce multiple wheel events in one client frame or render tick into one signed delta before rendering;
- apply the delta through a single shared tail-scroll helper and clamp once;
- keep the rendered pane slice, scrollbar thumb, hint state, and cursor visibility on the same post-clamp offset;
- when live PTY output arrives while `scrollback_offset != 0`, do not reset the operator's scrollback view unless the backing content is invalidated in a defined way.

**Verification:** add a capsule test that feeds a burst of same-direction wheel events and asserts the visible top row and `scrollback_offset` move monotonically. Add a second test with interleaved PTY output while scrolled and assert the view does not flicker back to live tail until the operator scrolls or jumps there.

**Fix:** audit the capsule pane scroll path:

- content-width calculation for pane rows/transcript;
- content-height calculation for scrollback rows/transcript;
- viewport-width calculation after borders and pane chrome;
- viewport-height calculation after borders and pane chrome;
- horizontal `scroll_x` storage and clamping;
- vertical scrollback offset storage and clamping;
- scrollbar thumb length/position calculation;
- keyboard/wheel/mouse input that changes horizontal scroll;
- keyboard/wheel/mouse input that changes vertical scrollback;
- resize/layout-change invalidation.

Prefer existing shared helpers such as `render_scrollable_block`, `render_horizontal_scrollbar`, `effective_offset`/clamp helpers, and fixed-prefix scroll helpers where they fit. If capsule panes need a lower-level primitive because they render through terminal-grid patches instead of Ratatui blocks, extract a reusable helper from the working implementations rather than keeping capsule-only math.

**Verification:** add capsule pane tests that:

- render a long line wider than the pane and assert horizontal overflow is detected;
- scroll horizontally and assert the visible slice changes;
- add more lines and assert the thumb still represents the current content/viewport;
- resize the pane/terminal and assert `scroll_x` clamps and the thumb recomputes;
- scroll vertically in scrollback mode and assert the visible top row matches the offset represented by the thumb;
- resize while in scrollback mode and assert the vertical offset clamps and the thumb still maps to the visible content;
- compare thumb position/length against the shared helper's expected geometry.

Add debug telemetry for this class of issue before changing behavior: on every pane scroll/render, log focused pane id, agent label, `alternate_screen`, content/scrollback length, viewport rows/cols, tail offset used by the renderer, scrollbar thumb start/len, visible slice start row, and cursor visibility decision. The existing log proves the wheel input and PTY feed state separately, but it does not yet print the exact scroll state consumed by the renderer in the same frame.

### Capsule Debug Info Hover / Copy Geometry Is Misaligned

In the capsule Debug info dialog, copy itself can work, but hover and click target the wrong row. Hovering/clicking the `Run ID` row can change/copy the `Container ID` row instead, and copied feedback appears on `Container ID` after interacting with the `Run ID` line.

That means the rendered row geometry and the interaction geometry disagree. The shared component's copy/hover logic is only correct if every caller passes the exact same dialog rectangle to rendering, hover hit-testing, click hit-testing, hyperlink overlay, and copied-row feedback.

**Decision:** row index is a shared geometry contract. The row that visually changes color, the row that receives `Copied!`, and the row whose payload is copied must be the same row under the pointer. A one-row offset is a blocker because it makes copy affordances untrustworthy.

**Fix:** audit the capsule path for every place it computes or passes the Debug info rect:

- rendering snapshot / `render_container_info`;
- `copy_payload_at`;
- hover update / `set_container_info_hover`;
- click handling;
- OSC 8 hyperlink overlay;
- footer hint axes/clamping.

All of them must use the same area and the same inner-body coordinate assumptions. If the capsule wraps the shared component in a Ratatui snapshot or raw-ANSI frame, the translation from screen row/column to component row/column must account for the dialog border, leading spacer, and any bottom chrome offset exactly once.

**Verification:** add capsule tests that compute the visible coordinates for `Run ID`, `Container ID`, and `Diagnostics log`, hover and click each value, and assert the copied row/payload matches the visible row. Include the scrolled-horizontal case so hit-testing follows the value slice after scrolling.

### Build Log Overlay Must Not Close On Inside Click

The Docker build-log overlay currently disappears when the operator clicks inside the overlay. It should not. The operator should close it with the advertised close keys (`Esc` and `q`); ordinary clicks inside the log body are no-ops unless they are scrollbar interactions.

The general modal lifecycle rule in `docs/content/docs/reference/tui/dialogs.mdx` already says:

- clicking outside a dialog dismisses it like Esc;
- clicking inside the dialog body where there is no interactive element is a no-op;
- `InsideSwallow` and `InsideHit` do not propagate to underlying chrome.

The current launch implementation violates that rule. In `crates/jackin-launch/src/tui/subscriptions.rs`, the `v.build_log_open` mouse-down path starts scrollbar dragging when the click is on the scrollbar, but otherwise sends `LaunchMessage::BuildLogClosed`. That makes any body click close the overlay.

**Decision:** the build-log overlay closes on keyboard only (`Esc`/`q`). Mouse inside the overlay must not close it. Scrollbar clicks/drags scroll. Wheel events scroll. Plain body clicks are swallowed. Underlying footer/status click targets must not fire while the overlay is open.

**Fix:** change the `v.build_log_open` mouse-down path:

- if click is on the scrollbar, start drag/update scroll;
- else if click is inside the build-log overlay/body, swallow with no state change;
- do not emit `BuildLogClosed` for ordinary clicks.

**Documentation update needed:** `docs/content/docs/reference/tui/chrome.mdx` currently says the build-log overlay is dismissed with `Esc`/`q` or a click. Update it to match the intended behavior: `Esc`/`q` close; click is only for scrollbar/interactive targets and otherwise no-op.

**Verification:** add a launch subscription test that opens the build-log overlay, sends a left-click in the log body, and asserts `build_log_open` remains true. Keep/extend tests for scrollbar drag and `Esc`/`q` close.

### Git Repository Prompt Must Use Canonical Dialog Padding

The `Git repository detected` prompt shown over the file browser has wrong top padding. The prompt content starts immediately below the top border:

```text
┌ Git repository detected ─────────────────────────────┐
│                          What would you like to do?  │
│     https://github.com/...                           │
│                                                      │
│        Mount this repository ...                     │
│                                                      │
└──────────────────────────────────────────────────────┘
```

The TUI reference already defines the standard in `docs/content/docs/reference/tui/dialogs.mdx`:

- Every dialog with content plus a button/action row uses the canonical five-slot inner layout.
- Exactly one blank leading spacer row sits between the top border and first content row.
- Exactly one blank spacer row sits between content and buttons.
- Exactly one blank trailing spacer row sits between buttons and bottom border.
- Implementations should use `jackin_tui::components::dialog_inner_chunks(inner, Some(content_rows))`.

The same roadmap checklist already names this exact offender, but marks it `[x]`: `Git repository detected` was identified as content flush to top with bottom spacer present and top spacer missing. The screenshot shows that the issue is still present.

**Decision:** this prompt is not a filter/list exception. It is a content + button dialog, so it must use the symmetric dialog padding standard.

**Fix:** update `crates/jackin-console/src/tui/components/file_browser/git_prompt.rs` so `render_git_prompt` uses the shared five-slot layout:

- Content slot: prompt plus optional URL.
- Spacer slot: the standard blank row.
- Action slot: the three-button row.
- Leading and trailing spacer slots: left blank.

Also reconcile the current height mismatch: `git_prompt_rect` computes `8`/`7` rows with/without URL, while `render_git_prompt` currently renders `7`/`6`. The renderer and hit-testing rect must agree.

**Verification:** add/adjust a render test that asserts the row immediately below the top border is blank, the prompt starts on the following row, the buttons remain separated by one spacer row, and the URL click rect still points at the URL row.

### File Browser Gutter Must Not Shift Behind Child Dialog

Before the `Git repository detected` child dialog opens, the file-browser listing uses a stable left gutter:

```text
│  ../
│▸ blockchain-nodes/ (git)
│  blockchain-nodes-ci-consistency/ (git)
```

After the child dialog opens, the parent list suppresses the active cursor, but it also removes the reserved left spacing:

```text
│../
│blockchain-nodes/ (git)
│blockchain-nodes-ci-consistency/ (git)
```

That shift is incorrect. The parent browser may dim its border and hide the active selection marker while a child dialog owns focus, but the row text column must not move. The operator should perceive the child dialog as stacked on top of the same parent screen, not as a reflowed parent.

**Decision:** selection cursor visibility and row layout are separate concerns. Hiding the `▸` glyph must not collapse the cursor/gutter column. Every file-browser row keeps the same left text start whether the child prompt is closed or open.

**Fix:** update `crates/jackin-console/src/tui/components/file_browser/render.rs` so the listing always reserves the cursor symbol width. When `pending_git_prompt` is open, hide or neutralize the cursor glyph/highlight, but keep the same gutter. The current implementation sets `show_cursor = false` and only calls `highlight_symbol("▸ ")` when it is true; that likely collapses the reserved symbol column despite `HighlightSpacing::Always`.

**Verification:** adjust `git_prompt_background_suppresses_browser_cursor_and_active_border` or add a new render test that compares the row text start column before and after opening the git prompt. The text start column must be identical while the visible active cursor glyph is absent behind the child dialog.

### Auth Source Rows Must Reserve Cursor Space

The workspace editor Auth tab currently renders the Claude Code source row without the same cursor gutter used by the neighboring selectable rows:

```text
│▸ Mode        sync (inherited)
│Source folder default: ~/.claude (CLAUDE_CONFIG_DIR)
│
│  + Override for a role
```

That makes the source label start too far left and leaves no room to display the selection cursor cleanly. The cursor must remain visible when the source/source-folder row is selected, and the text column must stay stable when selection moves between `Mode`, `Source folder`, and `+ Override for a role`.

**Decision:** auth source rows are not a layout exception. Selectability, cursor visibility, and row alignment must be handled by the same shared selectable-row primitive or cursor-gutter helper used by the rest of the Auth list.

**Fix:** audit `crates/jackin-console/src/tui/screens/settings/view.rs`, especially `render_auth_source_line` and `render_auth_source_folder_line`, plus the workspace editor Auth renderer if it has a separate path. Make every selectable Auth row reserve the same two-cell cursor gutter, rendering `▸ ` only for the selected row and `  ` for unselected rows.

**Verification:** add render coverage for the Auth tab with the source/source-folder row selected and unselected. Assert the selected row shows `▸`, and the label start column is identical for `Mode`, `Source folder`/`Source`, and `+ Override for a role`.

### Creation Sentinel Rows Must Share One Action Style

The workspace editor Mounts tab shows `+ Add mount` with the correct action-row color and selection effect. The workspace list shows `+ New workspace`, but it should read as the same kind of control: a `+ ...` creation sentinel at the bottom of a list.

These rows must not drift by surface:

```text
│  + Add mount
│
│   + New workspace
```

The operator should be able to learn one rule: `+ ...` means "create/add a new thing here." The color, weight, selected effect, cursor-gutter behavior, and disabled/available styling must be the same everywhere this pattern appears.

The TUI visual-design reference already defines this under **Action Rows (`+ Add ...` / `+ Override ...`)**: every row that begins with `+` renders through one shared `action_row_style(selected)` function in `jackin-console`; every `+ ...` row uses the same `ACTION_ACCENT` foreground; selected rows are bold; unselected rows are normal weight; no surface hand-rolls its own style.

**Decision:** `+ New workspace`, `+ Add mount`, `+ Override for a role`, and every other add/new sentinel are the same component pattern. A workspace-list sentinel is not special just because it lives in the sidebar.

**Fix:** audit every row label that begins with `+ ` across workspace list, editor tabs, settings tabs, Auth rows, environment rows, mount rows, and picker creation sentinels. Route them through the shared action-row styling helper or extract a more explicit creation-sentinel helper if the current helper cannot cover row construction, cursor gutter, and selected state consistently.

**Verification:** add or update render tests/snapshots comparing `+ New workspace` and `+ Add mount` in selected and unselected states. Assert foreground style, boldness, row prefix/gutter, and selected-row effect match the shared action-row contract.

### Error Dialog Content-To-Button Spacing Must Be One Row

The `Load role failed` dialog currently renders two blank rows between the final message line and the `OK` button:

```text
│                    Repository is not available, or you do not have access.                   │
│                                                                                              │
│                                                                                              │
│                                              OK                                              │
│                                                                                              │
```

The TUI dialog standard says there is exactly one blank spacer row between content and the action/button row. This must be consistent for every single-action error dialog and the lookbook story.

**Decision:** `ErrorDialog` is the canonical shared single-button error surface. The fix belongs in `crates/jackin-tui/src/components/error_dialog.rs`, not in the `Load role failed` caller. All uses in console, launch, runtime tests, and `jackin-tui-lookbook` should inherit the same layout.

**Likely cause:** `ErrorDialog::render` computes `body_rows = inner.height.saturating_sub(4)` and gives the whole remaining inner area to the body. For a short message inside a taller popup, the centered/wrapped paragraph leaves extra blank body rows before the canonical spacer, so the visible gap becomes two or more rows even though the layout declares one spacer.

**Fix:** size the body slot from the actual estimated message rows, capped by available space, and reserve exactly the canonical slots around it:

- leading spacer: 1 row
- message body: estimated wrapped message rows
- spacer before `OK`: 1 row
- `OK` action row: 1 row
- trailing spacer: 1 row

For overflow cases, keep the existing wrapping/scroll behavior consistent with the dialog standard instead of silently clipping.

**Verification:** add a shared `ErrorDialog` render test that locates the last non-empty message row and the `OK` row and asserts there is exactly one blank row between them. Confirm the lookbook `error/default` story uses the corrected shared component without a local override.

### Terminal Performance Claims Need Measurements

The PR and roadmap claim terminal performance improvements, but the `jackin-term` benchmark target referenced in the PR description does not exist and no real session measurements are captured.

**Decision:** performance-driven architecture claims need measured support. If measurements are deferred, the roadmap should say so plainly.

**Fix:** add the missing benchmark/measurement path or downgrade the acceptance status to partial with a tracked follow-up.

## Execution Strategy

Use this order unless investigation proves a dependency points elsewhere. The goal is to fix shared causes before polishing individual symptoms.

1. **Stabilize the spec and references.** Read the TUI docs listed at the top, settle the spec gaps, and update docs/lookbook references as implementation decisions become final.
2. **Finish architecture cleanup first.** Delete orphaned migrated code, move tests to owning crates where needed, remove low-level dependencies on TUI, and consolidate lint policy. This reduces the chance of editing dead paths.
3. **Unify Debug info.** Audit all Debug info entry points and force them through the shared model/renderer/state path. Fix version/log wiring, backdrop, footer hints, scroll, hover, copy, and hit-testing as one component-level change.
4. **Unify scroll primitives.** Fix dialog scroll and panel scroll through shared geometry/state. Scrollbar visibility, hint axes, input routing, hit tests, and clipping must all consume the same overflow facts.
5. **Fix capsule pane chrome around the PTY body.** Preserve the streaming terminal body unless a change is required for shared scroll/chrome correctness. Move reusable shell/chrome pieces into `jackin-tui` if direct `render_scrollable_block` reuse is not possible.
6. **Normalize dialog and selectable-row layout.** Apply five-slot padding to action dialogs and stable cursor gutters to file browser/Auth rows through shared helpers.
7. **Update lookbook and snapshots.** Any shared component visual change must be reflected in `crates/jackin-tui-lookbook` / `docs/public/tui-lookbook` and the related docs pages.
8. **Run full verification.** Do not mark rows complete until code, tests, docs, and visual references agree.

### Implementation Best Practices For The Goal Run

- Work in small, reviewable clusters, but keep the design contract global. For example, a Debug info fix should land as "shared Debug info behavior fixed everywhere," not "launch fixed, capsule still custom."
- Add or update tests before risky refactors when existing behavior is unclear. Render tests and geometry tests are especially valuable for TUI regressions.
- Prefer pure row-building/layout helpers reused by render, input, and tests. If input and rendering compute widths or rects separately, the next scroll/click bug is already seeded.
- Avoid changing the PTY stream body unless required. The requested capsule work is mostly chrome, focus, scrollbar, and retained-scrollback state.
- Treat docs as part of the implementation, not a follow-up. If a rule changes, the docs change in the same cluster.
- Keep operator-facing behavior exact: no hidden shortcuts, no false clickable icons, no false scrollbars, no row shifts, no silent truncation.

## Fix Queue

### Required Before Merge

| Priority | Status | Fix | Verification |
|---:|---|---|---|
| 1 | pending | Run `cargo fmt` to fix current formatting failures | `cargo fmt --check` |
| 2 | pending | Fix the container info label/test mismatch | `cargo nextest run --workspace` |
| 3 | pending | Delete orphaned migration-residue source files | `cargo check -p jackin` and targeted workspace checks |
| 4 | pending | Resolve CI/DCO failure | `gh pr checks 495` |
| 5 | pending | Correct roadmap and docs statuses that claim skipped work is done | docs build/link/type/test gates from `docs/` |

### Architecture Follow-Ups In Scope Unless Deferred Explicitly

| Status | Fix | Notes |
|---|---|---|
| pending | Move extracted tests into owning crates | Especially config-related tests currently living through re-export shims |
| pending | Remove `jackin-diagnostics -> jackin-tui` dependency | Move tiny helpers lower in the stack |
| pending | Reconcile closed-enum dispatch count | Current count is about 60 arms, not the documented ~17 |
| pending | Hoist lint tables into `[workspace.lints]` | Avoid 17 drifting manifest copies |

### TUI Follow-Ups In Scope Unless Deferred Explicitly

| Status | Fix | Notes |
|---|---|---|
| pending | Adopt `HoverTracker` on remaining surfaces | Avoid old scattered hover state beside new primitive |
| pending | Adopt `FocusOwner` on remaining surfaces | Avoid duplicated focus ownership state |
| pending | Adopt shared click/modal lifecycle where applicable | Capsule is currently a likely exception |
| pending | Unify remaining scroll models or extract missing shared helpers | Console, launch, capsule, dialogs, and lookbook must not carry duplicate scroll math or static scroll hints |
| pending | Audit every scrollable component for reusable helper usage | Check render, input, hit-testing, resize, hints, hover/copy overlays, and tests against `jackin_tui::scroll`, `scrollable_panel`, and `dialog_layout` |
| pending | Track capsule ANSI-to-Ratatui migration | Required if not completed in this PR |

## Verification Gates

Run these before asking to merge:

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

Also check:

```sh
gh pr checks 495
```

## Decisions To Settle In The PR

- Pick one canonical Debug info version label and make code, docs, tests, and lookbook match. The current operator-facing screenshots use `jackin version`; changing it requires an intentional docs/test update, not mixed output.
- For every deferred roadmap item, either complete it in this PR or re-status it as partial/follow-up with exact remaining scope. Do not leave roadmap checkboxes claiming done while this file says pending.
- Decide whether capsule chrome/pane scroll migration can fully land here. If not, land the reusable helper extraction that this PR needs and create/update a roadmap item for the remaining capsule migration with concrete files and behaviors.
