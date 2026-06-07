# PR #495 TUI Smoke Findings

This file is the handoff checklist for the remaining TUI issues found during the
operator's manual `--debug` smoke of PR #495 on branch `feature/tui-architecture`.

Use this as the target for a follow-up `/goal` command:

```text
/goal Follow PR-495-TUI-SMOKE-FINDINGS.md and fix all findings.
```

## Executive Summary

Fix the root causes, not ten isolated screenshots. The findings collapse into
four work groups:

1. **Shared Debug info/dialog contract is incomplete.** The row model exists, but
   render shell, footer/status preservation, hover, copy affordances, scroll, and
   hints are still wired per surface.
2. **Modal/footer layering is fragmented.** Capsule and launch still have
   full-screen blank-backdrop paths that cover status/footer chrome; console is
   closer to the intended reserved-footer model.
3. **Capsule scrollback/scrollbar rendering is unstable.** Pane scrollbar
   visibility is gated on current scrollback offset, and wheel scrollback emits
   full redraws even for saturated no-op events.
4. **Scrollable pane text selection is incomplete.** Selection copies on
   mouse-up, but the visible selection disappears immediately, there is no copied
   feedback, and drag selection does not auto-scroll beyond the viewport.

The correct outcome is a smaller number of shared primitives with stronger tests,
not more per-surface special cases.

## Execution Checklist

This checklist manages the whole goal. Work the items strictly one by one, in
the order below (it follows the Ordered Fix Plan phase order): implement the
fix, run its focused tests, capture the live evidence the finding requires,
and only then tick the box and move to the next item. Do not batch checkmarks
and do not start a later item while an earlier one is unverified.

Every checkmark here must propagate immediately — in the same commit — to the
matching Defect 64 box in
`docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`,
with the evidence note (command output and/or `--debug` run id) pasted there
per the Defect 54 pattern. When a tick changes the program status story
(first item done, last item done, a phase fully closed), update the Round 3 /
Defect 64 wording in
`docs/content/docs/reference/roadmap/post-restructure-fixes.mdx` in the same
commit too. A box checked here but not propagated is an incomplete item.

- [ ] F6 — Run ID / Diagnostics log semantics correct across all builders
  (Phase 1; shared contract tests in `jackin-tui`).
- [ ] F9 — Debug info shared interaction contract: copy affordances, hover,
  both-axis hit-tests, hyperlink overlay (Phases 1, 3).
- [ ] F4 — capsule Debug info preserves status bar and reserved chrome
  (Phase 2).
- [ ] F5 — one shared Debug info shell across console, launch, and capsule
  (Phase 2).
- [ ] F7 — every status-preserving dialog computes rects against the content
  area; reserved rows never covered (Phases 2, 3).
- [ ] F1 — build-log overlay bottom chrome on the shared hint/spacer/status
  stack (Phase 3).
- [ ] F8 — text-input prompts and remaining dialog families on the shared
  dialog system, or documented exceptions (Phase 4).
- [ ] F2 — pane scrollbar gates on real scrollability, shared scroll math
  (Phase 5).
- [ ] F3 — no-op wheel events skip redraw; scrollback movement leaves the
  clear-tier (Phase 5).
- [ ] F10 — persistent content-coordinate pane selection with copied feedback
  and edge auto-scroll (Phase 6).
- [ ] TUI docs updated with the proven contracts (Phase 7; run the docs
  gates).
- [ ] Convergence metrics from the refactor map hold on fresh sweeps
  (app-wide definition of done).
- [ ] Final re-smoke: one `--debug` session exercising all ten findings; run
  id and key log excerpts recorded here and in Defect 64.

**Implementation evidence captured before final smoke (2026-06-08):** code fixes
now cover F1/F2/F3/F4/F5/F6/F7/F9 at the focused-test level and advance F10's
copy-persist/clear/edge-drag behavior. The boxes above intentionally remain
open until the remaining convergence audit and a fresh `--debug` run id exercise
the real capsule/launch surfaces. Focused verification run so far:

- `cargo test -p jackin-tui container_info --locked` — 8 passed.
- `cargo test -p jackin-launch container_info --locked` — 4 passed.
- `cargo test -p jackin-launch build_log --locked` — 11 passed.
- `cargo test -p jackin-capsule container_info --locked` — 20 passed.
- `cargo test -p jackin-capsule debug_dialog_keeps_status_bar_visible --locked` — 1 passed.
- `cargo test -p jackin-capsule apply_action_wheel --locked` — 2 passed.
- `cargo test -p jackin-capsule scrollbar --locked` — 5 passed.
- `cargo test -p jackin-capsule selection --locked` — 19 passed.
- `cargo test -p jackin-tui labeled_text_input_dialog --locked` — 1 passed.
- `cargo test -p jackin-tui text_input_prompt_rect --locked` — 1 passed.
- `cargo test -p jackin-capsule rename_tab --locked` — 5 passed.

## Ground Rules

- Stay on `feature/tui-architecture`; do not create a new branch.
- Do not rewrite history or force-push without explicit operator authorization.
- Do not remove `cargo-audit`.
- Sign off any new commit with `git commit -s` and push immediately when the fix
  is complete. Per `AGENTS.md` Commit Attribution, every agent-authored commit
  also carries the agent's `Co-authored-by` trailer (for Claude Code:
  `Co-authored-by: Claude <noreply@anthropic.com>`; for Codex:
  `Co-authored-by: Codex <codex@openai.com>`).
- Use Conventional Commits types: findings fixes are `fix:`/`refactor:` with
  `test:` and `docs:` where the change is test-only or docs-only.
- Do not mutate host-side user state silently. Use the PR-scoped config/home
  paths from the PR template (see "Environment And Smoke Harness" below).
- Do not mark roadmap/checklist boxes `[x]` from inspection. A completed
  checklist item must include command output or diagnostics run ids.
- Defect 54 hardware/session checks stay `[ ]` until real run ids are captured.

Current observed checkout while this file was last extended:

- Branch: `feature/tui-architecture`
- Local head: `8b92b3c56611c8670afd41ca6bfd9bc1435a9beb`
  (`docs: add PR 495 TUI smoke handoff` — the commit that first added this file;
  later extensions of this file may still be uncommitted in the working tree)

The branch may advance. Before fixing, fetch and verify the current branch/head
rather than assuming this exact commit is still current.

## Source Of Truth

Read these before fixing:

- `AGENTS.md`
- `.github/AGENTS.md` — jackin-capsule smoke-test mandate, PR merge rules
- `PULL_REQUESTS.md` — Verify-locally block shape
- `TESTING.md` — nextest invocation conventions
- `docs/content/docs/reference/roadmap/post-restructure-fixes.mdx`
- `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`
- `docs/content/docs/reference/tui/index.mdx`
- `docs/content/docs/reference/tui/navigation.mdx`
- `docs/content/docs/reference/tui/dialogs.mdx`
- `docs/content/docs/reference/tui/components.mdx`

The canonical TUI design docs already exist under `docs/content/docs/reference/tui/`.
Extend those pages only after the implementation and verification prove the new
Debug info/dialog contract. Do not create a parallel TUI design page.

## Environment And Smoke Harness

Everything in this section is prerequisite for reproducing any finding and for
capturing the Defect 54 evidence.

### PR-scoped host paths

jackin' resolves its host directories from two environment variables, read in
`crates/jackin-core/src/paths.rs:33-34`:

- `JACKIN_HOME_DIR` — overrides `~/.jackin` (data, cache, roles, diagnostics).
- `JACKIN_CONFIG_DIR` — overrides `~/.config/jackin` (config.toml, workspaces).

The PR template (`.github/PULL_REQUEST_TEMPLATE.md`, Checkout block) isolates
smoke runs with:

```sh
export JACKIN_CONFIG_DIR="$JACKIN_PR_TEST_DIR/.config/jackin"
export JACKIN_HOME_DIR="$JACKIN_PR_TEST_DIR/.jackin"
```

The evidence run for this file used `JACKIN_HOME_DIR=/Users/donbeave/.jackin-pr-495`
(hence the `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/...` paths in
"Run Evidence"). Reuse the same root for follow-up smokes so old and new run ids
sit side by side.

### Capsule binary resolution trap (read before any capsule-side fix)

The capsule binary inside the container is resolved by `ensure_available` in this
order (`.github/AGENTS.md`, jackin-capsule section):

1. `JACKIN_CAPSULE_BIN=/path` env override — used directly, no cache.
2. Cache hit at `~/.jackin/cache/jackin-capsule/<version>/linux-<arch>/`.
3. Download from the GitHub preview/rolling release tag.

**If you change any `crates/jackin-capsule/` code and smoke without exporting a
freshly built binary, the container will happily run the cached or released
capsule and your fix will be invisible.** Findings 2, 3, 4, 5, 9, and 10 all
have capsule-side components. Before every capsule smoke run:

```sh
eval "$(cargo run --bin build-jackin-capsule -- --export)"
```

This one-shot build prints and `eval`-exports `JACKIN_CAPSULE_BIN` pointing at
the just-built Linux binary. Re-run it after every capsule-side code change.

### Smoke commands

Every smoke invocation must include `--debug` (AGENTS.md rule; it captures
external commands and `[jackin debug ...]` instrumentation into the run JSONL
and prints the run id at start). Debug mode already suppresses the intro — do
not add `--no-intro`.

```sh
# Console-first smoke (preferred):
cargo run --bin jackin -- console --debug

# Load-path smoke (only when the finding needs the load CLI path):
cargo run --bin jackin -- load the-architect . --debug
```

Prefer the `the-architect` role over `agent-smith` when a role choice is needed.

### Snapshot/test infrastructure notes

- `insta` (with the `filters` feature) is a dev-dependency of `crates/jackin`
  and `crates/jackin-capsule`. `crates/jackin-tui` currently asserts on rendered
  `Buffer` contents manually (see `container_info/tests.rs`); either pattern is
  acceptable for the new tests — follow the crate you are editing.
- `dind_e2e` tests are feature-gated (`#![cfg(feature = "e2e")]` at
  `crates/jackin/tests/dind_e2e.rs:7`). `--all-features` enables that feature,
  which is why the local verification command excludes the binary explicitly
  (see Phase 8). CI runs the full suite only after building the capsule and
  exporting `JACKIN_CAPSULE_BIN` (`.github/workflows/ci.yml`, nextest job).

## Run Evidence

Operator smoke run:

- Run id: `jk-run-533476`
- Host diagnostics JSONL (3,099 lines):
  `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.jsonl`
- JSONL `container_started` event (line 3078) points to capsule log
  (139,563 lines, ~42 MB):
  `/Users/donbeave/.jackin-pr-495/data/jk-paje1he3-thearchitect/state/multiplexer.log`
- Docker build log (clean build, exit 0, all 53 stages OK, no warnings):
  `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.docker-build.log`

### How to read the run JSONL

Each line is one event: `{"ts_ms", "run_id", "trace_id", "kind", "message"}`
plus optional `"detail"`, `"stage"`, `"span_id"`. **`detail` is a JSON-encoded
string, not a nested object** — quotes inside it are escaped, so a plain
`rg '"capsule_log"'` matches nothing (the raw bytes are `\"capsule_log\"`).
That is why an earlier helper printed an empty `capsule_log=` field even though
the pointer exists. Use `jq ... | fromjson` instead:

```sh
RUN_JSONL=/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.jsonl

# Capsule log path (works for any run id):
jq -r 'select(.kind=="container_started") | .detail | fromjson | .capsule_log' "$RUN_JSONL"

# End-of-run summaries (event counts, stage durations, exit reason):
jq -c 'select(.kind=="run_summary" or .kind=="exit_summary")' "$RUN_JSONL"

# Raw greps still work for non-detail fields:
rg -n 'capsule_log|error|panic|resize|render|dialog|scroll|mouse|key' "$RUN_JSONL"
rg -n 'bottom-chrome|wheel dispatch|scrollback-movement|render: kind=full|render: kind=partial' \
  /Users/donbeave/.jackin-pr-495/data/jk-paje1he3-thearchitect/state/multiplexer.log
```

### Quantified log facts (verified against the files above)

| Fact | Value | Where |
| --- | --- | --- |
| `bottom-chrome: site=ratatui` lines | 6,850 | multiplexer.log |
| `bottom-chrome: site=raw-full` lines | 6,850 (1:1 paired with ratatui) | multiplexer.log |
| `render: kind=full reason=scrollback-movement` | 3,442 | multiplexer.log |
| `render: kind=partial` | 7,343 (all `reason=pty-output`) | multiplexer.log |
| `wheel dispatch: jackin-scrollback` lines | 6,882 (= ~3,441 events; each event logs a `before=` and an `after=` line) | multiplexer.log |
| Wheel events → full redraws ratio | ≈1:1 (3,441 events vs 3,442 full redraws) | derived |
| `cockpit-dialog-mouse` events | 2,700 (run_summary `event_counts`; first occurrences at JSONL lines 72–74 with `container_info_open=true`) | jk-run-533476.jsonl |
| Errors/panics in either log | 0 | both |
| `container_started` event | JSONL line 3078 | jk-run-533476.jsonl |
| `exit_summary` / `run_summary` | JSONL lines 3090 / 3091 ("1 agent still in the Construct; boundary outro skipped") | jk-run-533476.jsonl |

Saturated no-op wheel event followed by a full redraw and both chrome sites
(multiplexer.log lines 2476–2480):

```text
2476: wheel dispatch: jackin-scrollback session=2 row=25 col=86 button=64 delta=3 before=9 filled=9
2477: wheel dispatch: jackin-scrollback session=2 after=9
2478: render: ratatui-frame damage=full panes=1 pane_screens=1
2479: bottom-chrome: site=ratatui term=149x39 frame_area=149x39 hint_y=36 sep_y=37 branch_bar_y=38 panes=1
2480: bottom-chrome: site=raw-full term=149x39 branch_bar_row=38 hint_row=36 debug_chip=jk-run-533476
```

Paired ratatui + raw-full chrome draw in the same frame family (lines 2415–2416):

```text
2415: bottom-chrome: site=ratatui term=149x39 frame_area=149x39 hint_y=36 sep_y=37 branch_bar_y=38 panes=1
2416: bottom-chrome: site=raw-full term=149x39 branch_bar_row=38 hint_row=36 debug_chip=jk-run-533476
```

Observed log facts in prose:

- The run JSONL does contain a capsule log pointer (line 3078) even though the
  helper printed an empty `capsule_log=` field — see the JSON-encoded `detail`
  note above for why the naive grep missed it.
- The capsule log records `bottom-chrome: site=ratatui` and
  `bottom-chrome: site=raw-full` in lockstep (6,850 each), meaning chrome is
  drawn through two paths in the same frame family. A third site,
  `site=dialog`, exists in code (`crates/jackin-capsule/src/tui/view.rs:104`)
  for dialog-open frames.
- Scrollback wheel events produce `render: kind=full reason=scrollback-movement`
  essentially once per event (3,442 full redraws for ~3,441 wheel events),
  including saturated no-op wheel events where `before=9 after=9`.
- The run JSONL records 2,700 `cockpit-dialog-mouse` events, and the early log
  lines (72–74) show modal mouse move/scroll events with
  `container_info_open=true` during the launch cockpit dialog — the Debug info
  dialog was absorbing a mouse-move flood.

## Target Architecture

The fix must converge Debug info and dialog behavior across the whole application:

- One shared Debug info data model.
- One shared Debug info dialog renderer contract.
- One shared row order and label vocabulary.
- One shared scroll implementation for vertical and horizontal overflow.
- One shared copy/hover hit-test model.
- One shared hint/footer contract.
- Status bars remain visible when the underlying screen had a status bar before
  opening the dialog.
- Surface-specific code may only provide available facts and store surface-local
  state such as scroll offsets, copied row, and hovered row.

Known shared primitives that should be reused or extended:

- `crates/jackin-tui/src/components/container_info.rs` — `DebugInfo` model,
  `ContainerInfoState`, render + hit-test + hyperlink helpers
- `crates/jackin-tui/src/components/dialog_layout.rs` — `DialogBodyScroll`
  (line 44), `scroll_hint_spans()` (line 284)
- `crates/jackin-tui/src/components/hint_bar.rs` — `render_hint_bar()` (line 78)
- `crates/jackin-tui/src/components/status_footer.rs` — `render_status_footer()`
  (line 163; owns the standard hint/spacer/status stack)
- `crates/jackin-tui/src/components/modal_backdrop.rs` — `ModalBackdrop` (line 14)
- `crates/jackin-tui/src/components/text_input.rs` — `render_text_input()` (line 385)
- `crates/jackin-tui/src/components/scrollable_panel.rs`
- `crates/jackin-tui/src/scroll.rs` — shared scroll math incl.
  `ScrollState::scroll_by` (line 79) and `tail_vertical_thumb` (line 439)
- `crates/jackin-tui/src/geometry.rs` — `HintSpan` (line 32; note: the type
  lives here, not in hint_bar.rs)
- `crates/jackin-console/src/tui/components/modal_rects.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/components/modal_layout.rs` —
  `modal_outer_rect` (line 16)

Avoid adding parallel dialog/backdrop/scroll/copy implementations. If a helper is
missing, add it to the shared component layer and route all surfaces through it.

### Console crate duality (orientation note)

Two console trees exist and both are real: `crates/jackin-console` is the
extracted crate the `jackin` binary depends on
(`jackin-console = { path = "../jackin-console" }` in `crates/jackin/Cargo.toml`),
and `crates/jackin/src/console/` is the in-binary half that bridges to it (TUI
migration pending, per the AGENTS.md TUI code location table). Findings touch
both: e.g. `debug_run_info_state` lives in
`crates/jackin-console/src/tui/components/container_info.rs:11`, while the modal
layout used by the console lives in
`crates/jackin/src/console/tui/components/modal_layout.rs`. Do not "fix" the
duality in this goal; just put any new shared code in `crates/jackin-tui`.

### Known duplication hotspots (verified, DRY-rule targets)

- Two `render_footer` implementations:
  `crates/jackin-console/src/tui/view.rs:71` and
  `crates/jackin-launch/src/tui/components/footer.rs:27`.
- A capsule-local hint-span builder: `footer_hint_spans` at
  `crates/jackin-capsule/src/tui/components/dialog.rs:1317`.
- Two scroll-by implementations: shared `ScrollState::scroll_by`
  (`crates/jackin-tui/src/scroll.rs:79`) vs capsule
  `Session::scroll_by(i32)` (`crates/jackin-capsule/src/session.rs:673`,
  with `scrollback_filled` at line 705).
- Capsule `DialogBackdrop` is already a re-export of the shared widget
  (`pub use jackin_tui::components::ModalBackdrop as DialogBackdrop;` —
  `crates/jackin-capsule/src/tui/components/chrome.rs:195`), so the backdrop
  *widget* is shared; the bug is full-area usage, not a parallel widget.

## Canonical Debug Info Contract

All Debug info surfaces must use this contract. A surface may omit rows whose
data is unavailable; it must not change row order, labels, scroll behavior, copy
behavior, or footer/status behavior.

Canonical title:

- `Debug info`

Canonical row order (source of truth: `DebugInfo::into_state`,
`crates/jackin-tui/src/components/container_info.rs:122-145`):

1. `Container ID` — copyable
2. `jackin version`
3. `jackin-capsule`
4. `Role`
5. `Agent`
6. `Target`
7. `Run ID` — copyable
8. `Diagnostics log` — copyable + OSC 8 hyperlink

Canonical row semantics:

- `Run ID` is always the bare run id, for example `jk-run-b93735`. It is never a
  `.jsonl` path.
- `Diagnostics log` is the full diagnostics JSONL path. It is copyable and uses
  an OSC 8 `file://` hyperlink when the terminal path is known.
- `Container ID`, `Run ID`, and `Diagnostics log` are copyable whenever present
  (the `.copyable()` builder on `ContainerInfoRow`,
  `crates/jackin-tui/src/components/container_info.rs:54`).
- Other rows are informational unless a later shared contract explicitly marks
  them copyable.

Canonical copy affordance:

- Each copyable row must render an explicit, shared copy affordance on screen.
  The preferred shape is a small shared copy action/glyph/button aligned with the
  row value, plus footer text naming the active copy behavior. The weaker
  "click the value if you already know it is clickable" behavior is not enough.
- The value text itself remains clickable and hoverable.
- Hover feedback applies only to copyable value cells and the copy affordance,
  not the whole row label or blank dialog area.
- Keyboard copy must be consistent across surfaces. If Enter copies the currently
  selected/default copy target on one surface, every Debug info surface must
  expose the same model or the footer must explicitly describe the surface's
  different copy action. Prefer one shared model.

Canonical scroll behavior:

- Debug info uses both-axis dialog body scrolling.
- A horizontal scrollbar appears whenever any rendered row is wider than the
  viewport.
- A vertical scrollbar appears whenever row count exceeds the viewport.
- Scroll hints advertise only axes that actually overflow
  (`debug_info_hint_spans(axes)`,
  `crates/jackin-tui/src/components/container_info.rs:335`, fed by
  `scroll_hint_spans`, `crates/jackin-tui/src/components/dialog_layout.rs:284`).
- Copy hit-testing, hover hit-testing, copied-row feedback, and OSC 8 hyperlink
  overlays must follow both scroll axes.

Canonical status/footer behavior:

- If the underlying screen had a status/footer bar before Debug info opened, that
  status/footer bar remains visible.
- Debug info is centered inside the content area that excludes reserved
  status/footer rows.
- Hints live in the reserved footer/hint area. Do not render floating hint lines
  under the dialog.

## Canonical Scrollable Text Selection Contract

This contract applies to scrollable terminal/pane content where the operator can
drag-select displayed text and copy it to the clipboard. The pane content is not
editable through jackin's selection layer, so selection has exactly one purpose:
copying displayed text. It must not imply editing, deletion, replacement, paste,
or mutation of pane content.

External practice references:

- W3C Selection API defines selection as the mechanism that lets users select a
  portion of content for copy, paste, and editing operations, and models the
  selection as a range with direction that user action can change:
  <https://www.w3.org/TR/selection-api/>.
- W3C Selection API user interactions say user agents should allow users to
  change the active selection and must not clear a non-empty selection just
  because the user clicks a non-editable region:
  <https://www.w3.org/TR/selection-api/#user-interactions>.
- MDN describes `Selection` as the range selected by the user, with an anchor
  where mouse selection starts and a focus where the mouse is released:
  <https://developer.mozilla.org/en-US/docs/Web/API/Selection>.
- W3C Pointer Events lists native text selection as a normal default action of
  `mousedown`, alongside drag/drop and scroll/pan behavior:
  <https://www.w3.org/TR/pointerevents/#the-mousedown-event>.
- W3C WAI technique G149 points back to user-agent accessibility expectations
  that selection/focus states are visibly highlighted:
  <https://w3c.github.io/wcag/techniques/general/G149>.

Design rule:

- The content is read-only from the selection system's point of view. The only
  supported selection action is copy.
- Do not add edit/delete/replace/cut/paste behavior to this selection model.
  Keystrokes still go to the pane normally after clearing any persisted
  selection.
- A completed drag selection remains visibly selected after mouse-up.
- Mouse-up copies the selected text to the clipboard and shows visible feedback
  that the selection was copied.
- Copied feedback must be visible as a transient overlay/toast, not as noisy
  log output and not in the hint/footer row. The hint row is only for currently
  available actions.
- The persisted selection is cleared by an explicit deselect action:
  - click outside the selected range or on non-selectable chrome;
  - begin typing/sending input to the pane;
  - begin a new selection;
  - close/clear the pane/session;
  - use an explicit cancel/dismiss key if one is added and shown in the footer.
- A simple mouse-up after selecting must not clear the selection.
- The selection model stores anchor and focus positions in content coordinates,
  not screen coordinates, so it survives scroll offset changes and terminal
  redraws until explicitly cleared.
- Selection rendering follows the visible viewport: only visible selected cells
  are highlighted, but the stored range may extend outside the viewport.
- Drag selection inside a scrollable pane auto-scrolls when the pointer is held
  near or beyond the top/bottom viewport edge:
  - dragging upward scrolls toward older content and extends the selection;
  - dragging downward scrolls toward newer content and extends the selection;
  - auto-scroll stops at content bounds;
  - the scroll rate may be stepped or distance-sensitive, but it must be stable,
    bounded, and testable.
- Auto-scroll during selection must not route the same event to the pane PTY.
- Selection must coexist with normal scrollback:
  - wheel scrolling after a persisted selection keeps the selected range selected
    in content coordinates;
  - typing clears selection before sending the key to the pane;
  - clicking ordinary content outside the selected range clears selection unless
    that click starts a new selection.
- Footer/hints must advertise only available actions. Copy success feedback is
  state feedback, so it belongs in the transient overlay/toast layer instead.

## Binding Design Rules (verbatim)

These published rules are the pass/fail review criteria for findings 1, 4, 5, 7,
8, 9, and 10. Quoted from the docs so a reviewer can judge a diff without
opening the site:

From `docs/content/docs/reference/tui/dialogs.mdx`:

> "The reserved status/hint rows at the bottom of every screen are inviolable:
> no modal, dialog, or border may draw onto them."

> "All modal rects are computed against the **content area** (full terminal
> minus footer height), not the full terminal area."

> "The modal backdrop is rendered over the screen minus the reserved footer
> rows, so the modal-aware footer stays visible beneath the dialog."

> "A modal is not a special case — when one is open, its keys replace the screen
> keys in the **same reserved footer rows**. There is no floating hint bar under
> a dialog; hints always live in the fixed footer."

From `docs/content/docs/reference/tui/navigation.mdx`:

> "All keyboard hint text belongs in the footer bar at the bottom of the screen.
> Dialogs, modals, and overlays **must not** render their own internal hint
> line."

> "A full-screen overlay that hides the footer still renders its hint as the
> bottom row of the screen, outside its box — not as an internal line of the
> box."

> "A surface may show `↑↓ scroll` only when its content overflows vertically,
> `←→ scroll` only when it overflows horizontally" — derived from the "same
> `is_scrollable` gate the scrollbar uses".

> Clickable targets must look clickable: every element that performs an action
> on click must expose both the hand pointer on hover and a hover style change.

From `docs/content/docs/reference/tui/components.mdx`:

> "The 'Debug info' dialog is a single reusable component, not a per-surface
> reimplementation. Every surface that shows it — the console manager, the
> launch cockpit, and the in-container capsule — builds it from the shared
> `DebugInfo` model."

The console-side mechanism the docs name for footer preservation is
`prepare_visible_modal()` (`crates/jackin/src/console/tui/layout/prepare.rs:43`),
which subtracts `footer_height` before centering modals. Launch and capsule
have no equivalent today — that is the structural gap behind findings 4 and 7.

## Application-Wide Refactor Map

The ten findings are instances of six application-wide patterns. Per the
"Fix the whole class, never one instance" rule in
`docs/content/docs/reference/roadmap/post-restructure-fixes.mdx`, the fix for
each finding must migrate every site in the corresponding inventory below, not
only the site the operator's screenshot happened to show. This map is the
verified current-state inventory (five repo sweeps, 2026-06-08); treat each
list as the migration work-list for the matching phases.

### R1 — Dialog shell, backdrop, and chrome preservation (F1, F4, F5, F7, F8)

Scale: 47 dialog-like surfaces (capsule 9, console 35 including the
SettingsEnv/GlobalMount/SettingsAuth sub-modal families, launch 3) and
24 modal-geometry functions outside `jackin-tui` (console 15 in
`modal_rects.rs` + file-browser rects, launch 8 popup/failure rects, capsule 1
`Dialog::box_rect` at `dialog.rs:1255`).

Per-surface current state:

- Capsule: the 9 `Dialog` variants already render through one local shell
  (`render_dialog_ratatui`, `dialog_widgets.rs:337`) over shared
  `dialog_layout`/`Panel` widgets — the divergence is at the *frame* layer:
  `render_capsule_ratatui_frame` (`view.rs:234-239`) paints `DialogBackdrop`
  over `frame.area()` and returns before the top `StatusBarWidget`
  (`STATUS_BAR_ROWS = 2`, `status_bar.rs:52`), while the raw bottom chrome
  (branch bar + hints) is appended separately by the compositor via
  `render_capsule_dialog_bottom_chrome` (`view.rs:99`) unless
  `blank_background`. Split chrome ownership, not per-dialog drift.
- Console: every modal rect derives from `modal_rects.rs` +
  `prepare_visible_modal` (footer height subtracted — the model to
  generalize). One sweep flagged console modal hints as floating-internal,
  which contradicts the dialogs.mdx modal-aware-footer rule; verify per modal
  during the Phase 4 audit instead of trusting either claim.
- Launch: `dialog_backdrop()` (`dialog.rs:13`) owns the full frame and splits
  off a hint row itself; no status-footer preservation
  (`ErrorPopup`/`BuildLogDialog`/`FailurePopup`).

Target: one shared modal layer in `jackin-tui` that takes content area and
reserved chrome area as separate inputs, replaces the 24 local rect functions
with shared sizing (or thin per-dialog size hints), and removes the capsule
frame-layer early return.

### R2 — Bottom-chrome stack (F1, F7)

Scale: 6 footer renderers, 23 hint builders (17 const tables + 6 functions),
5 height constants, 16 render call sites (14 Ratatui + 2 raw ANSI), 8 local
bottom-row math sites.

Spacer-policy divergence (the F1 class):

- Console: compliant — `footer_height()` (`jackin-console/src/tui/view.rs:66`)
  enforces a +1 spacer row above hints; `render_footer` (`view.rs:71`) splits
  it explicitly.
- Capsule raw path: compliant — `CAPSULE_HINT_SEPARATOR_ROWS = 1`
  (`layout.rs:16`) encodes the same gap.
- Launch: divergent — main footer is a bare 1-row status bar
  (`footer.rs:27`); the build-log overlay composes a tight 2-row hint+status
  stack with no spacer (`build_log_dialog.rs:197-206`). This is the only
  surface violating the policy, and F1 is its visible symptom.

Target: one shared bottom-chrome stack primitive (hint rows + spacer +
status footer) built on `render_status_footer` (`status_footer.rs:163`) and
`render_hint_bar` (`hint_bar.rs:78`); launch adopts it; the capsule raw
emitter (`render_hint_row`, `dialog/hint.rs:120`) stays as the one documented
non-Ratatui adapter but derives row offsets from the same height constants.
Hint builders consolidate on `HintSpan` vocabulary (all 23 already are or can
be); floating-internal hint rows go to zero.

### R3 — Scroll unification (F2, F3)

Scale: 19 shared scroll APIs in `jackin-tui/src/scroll.rs` (including
`is_scrollable`:102, `TailScroll`:64, `mouse_scroll_delta`:167,
`offset_for_track_position`:335, `tail_vertical_thumb`:439) plus
`DialogBodyScroll` (`dialog_layout.rs:44`) and the scrollable-panel helpers;
12 files already consume them; ~35 scroll-state sites still hold local state;
11 wheel-dispatch sites (3 through shared helpers, 2 mutating scroll fields
directly, 6 local dispatch).

Migration buckets:

- Obvious-yes (mechanical): capsule dialog wheel arms mutate
  `scroll.scroll_x/scroll_y` directly (`input_dispatch.rs:344,354`;
  `dialog.rs:645-657`) — replace with `DialogBodyScroll::on_mouse_scroll*`;
  console `confirm_save.rs:46-158` single-axis offset → `DialogBodyScroll`;
  console `focus.rs:34-57` cursor-follow math → shared follow helper;
  `list_geometry.rs:35` manual clamp → `clamp_offset_u16`.
- Needs-adapter: console editor/settings/workspaces plan-based scrolling
  (`EditorScrollFocusPlan`, `SettingsScrollFocusPlan`,
  `WorkspaceListScrollFocusPlan` in the respective `update.rs`) — keep the
  plan shape, route offset arithmetic through shared helpers.
- Already shared, keep: capsule `Session::scroll_by`/`clamp_scrollback_offset`
  wrap `TailScroll`; launch build-log metrics call `scroll::max_offset`;
  lookbook uses `scroll_selectable_list`/`apply_mouse_scroll_u16`.
- F2's fix point: `apply_pane_scrollbar` gate (`view.rs:322-324`) switches
  from `offset > 0` to the same `is_scrollable` gate the hints rule requires.

### R4 — Mouse, hover, and copy interaction (F9)

Scale: 6 mouse-dispatch entry functions across 3 surfaces; 11 hover
implementations (1 shared `HoverTracker`, `hover_tracker.rs:21-97`, plus 10
surface-specific); 13 copy-related sites — OSC 52 encoding is already shared
(`encode_osc52_clipboard_write`, `jackin-tui/src/lib.rs:299`) but 8 trigger
sites and 4 `mark_copied` feedback wirings are per-surface; 7 hit-test
helpers (1 shared point-in-rect + 6 domain-specific); pointer-shape (OSC 22)
plumbing exists on all three surfaces (`PointerShape`, capsule
`app.rs:53-108`; console `mouse.rs:270-290`; launch
`subscriptions.rs:216-285`).

Target: a shared dialog-interaction controller in `jackin-tui` that owns
hover-row derivation, copy hit-testing, copied-badge state, and pointer-shape
selection for copyable rows, so console/launch/capsule mouse entry points
translate coordinates and delegate instead of re-implementing the
move/click/copy state machine three times.

### R5 — Pane text selection (F10)

Existing implementation (all capsule): `SelectionState`
(`tui/selection.rs:13-27`) stores anchor/end in 0-based grid coordinates
relative to the pane inner rect — screen-relative, exactly the F10 root
cause; lifecycle actions `StartSelection`/`SelectionMotion`/
`FinalizeSelection` (`input_dispatch.rs:563-569`,
`mouse_input.rs:213-274`); extraction `selection_text()`
(`selection.rs:52-86`); highlight `apply_selection_highlight()`
(`view.rs:204-220`) painted only during active drag; pointer-shape already
selection-aware (`pointer_shape_for_state`, `app.rs:86-108`).

Target: extend `SelectionState` to content coordinates (scrollback-absolute
rows), persist after `FinalizeSelection`, render the highlight from the
persisted range intersected with the viewport, surface copied feedback
through the shared status chrome (R2 stack), and add the edge-auto-scroll
ticker on top of the same `TailScroll` bounds wheel scrolling uses (R3).

### R6 — Redraw-tier classification (F3)

Verified architecture: all 15 `FullRedrawReason` variants (`update.rs:17`)
route through `compose_full_redraw` (`compositor.rs:56`), which calls
`terminal.clear()` (`compositor.rs:67`) — emitting `ESC[2J` and forcing full
recomposition for every non-PTY action including wheel scrollback, dialog
hover (`DialogChange` via `mouse_input.rs:52-53`), selection drag repaint,
focus change, and the status ticker. Only PTY output takes the partial path
(`compose_pending_frame` routing at `compositor.rs:26`;
`compose_direct_dirty_pane_frame` at `compositor.rs:431`). The bottom chrome
is cached (`last_bottom_chrome`, `compositor.rs:345`) and re-emitted only on
change, so the visible flicker comes from the unconditional clear, not from
chrome duplication. Console and launch render loops are plain full-frame
Ratatui draws relying on cell diffing — no clear, no flicker class.

Target tiers:

- Clear-tier (geometry truly invalidated): `FirstAttach`, `Resize`,
  `LayoutChange`, `SplitClose`, `ZoomChange`, `SessionExit`,
  `ExplicitRedraw` (operator Ctrl-L semantics).
- Diff-tier (recompose without `terminal.clear()`, let the cell diff emit the
  delta): `ScrollbackMovement`, `DialogChange`, `SelectionRepaint`,
  `PaletteOverlay`, `FocusChange`, `PaneClear`, `StatusChange`.
- No-op tier: saturated wheel events and hover moves that change no state
  skip composition entirely (the F3 before/after offset comparison).

### Convergence metrics (app-wide definition of done)

The refactor is complete when these counts hold, verified by fresh sweeps:

- Modal-geometry functions outside `jackin-tui`: 24 → 0, or each survivor
  carries a one-line structural justification comment.
- Floating-internal dialog hint rows: 0 (navigation.mdx rule holds
  everywhere).
- Bottom-chrome stacks: 6 renderers → 1 shared stack + the documented capsule
  raw adapter, both reading the same height constants.
- Direct mutations of scroll fields outside shared scroll methods: 2 → 0.
- Wheel handlers bypassing shared delta/clamp helpers: 0.
- `compose_full_redraw` callers that clear the terminal for diff-tier
  reasons: 0; saturated scrollback wheel events produce no frame.
- Debug info renderers: exactly 1 shared shell; per-surface code is fact
  assembly + state storage only.
- Pane selection stored in screen coordinates: 0 (content-coordinate model
  only).

## Root Cause Groups

| Group | Findings | Shared fix direction |
| --- | --- | --- |
| Debug info contract | 4, 5, 6, 7, 9 | Centralize Debug info shell, rows, copy affordance, hover, scroll, footer hints, and status preservation in `jackin-tui`; surface code only supplies facts/state. |
| Dialog/footer shell | 1, 7, 8 | Introduce or extend shared modal layer helpers that receive content area + reserved footer/status area separately. Remove full-frame backdrop paths from status-preserving surfaces. |
| Capsule scroll rendering | 2, 3 | Use shared scrollability math for pane scrollbar visibility and suppress/full-reduce redraws for no-op scrollback wheel events. |
| Scrollable text selection | 2, 3, 10 | Store pane selections in content coordinates, keep selection visible after copy, show copied feedback, auto-scroll during edge drag, and clear selection only on explicit deselect/input/new selection. |

## Dependency Map

- Lock the shared Debug info contract before replacing console/launch/capsule
  renderers.
- Fix footer/status layout before finalizing build-log spacing, because build-log
  spacing should use the same footer/hint stack.
- Fix scroll helper usage before changing pane scrollbar behavior, otherwise
  scrollbar visibility and wheel dispatch can drift again.
- Fix pane selection coordinate storage before implementing drag auto-scroll,
  because auto-scroll must extend the selection in content coordinates rather
  than screen coordinates.
- Fix no-op wheel redraw before collecting performance/log evidence.
- Update TUI docs only after code and tests prove the final contract.
- Update roadmap/checklist boxes only after the required command output or live
  run ids exist.

## What Not To Do

- Do not add another Debug info renderer.
- Do not hide status/footer rows to make centering simpler.
- Do not leave console, launch, and capsule with different Debug info behavior.
- Do not solve flicker by hiding scrollbars, dropping input events, or suppressing
  debug logs.
- Do not clear a completed text selection immediately after copying it.
- Do not implement pane selection in screen coordinates; it must survive
  scrolling and redraws until explicitly cleared.
- Do not mark Defect 54 done with headless tests or code inspection.
- Do not silently weaken copy requirements to "the value is clickable if the
  operator guesses it".
- Do not introduce a broad dialog rewrite that ignores the observed smoke path.
  Keep the refactor scoped to shared primitives needed by these findings.

## Evidence To Preserve And Re-check

Use these anchors while fixing:

```sh
rg -n 'render_capsule_ratatui_frame|render_container_info_on_blank|DialogRatatuiSnapshot::DebugInfo|render_build_log_dialog|wheel dispatch|scrollback-movement|render_text_input_dialog|Run ID|Diagnostics log' crates
rg -n 'Run ID.*jsonl|Run ID.*diagnostics|render_container_info_on_blank|bottom-chrome: site=raw-full|bottom-chrome: site=ratatui' crates
```

Post-fix evidence expectations:

- No status-preserving surface should call a full-frame blank Debug info renderer.
- A `Run ID` row must never contain `.jsonl`.
- Saturated scrollback wheel events must not create `render: kind=full
  reason=scrollback-movement` — in a post-fix smoke log, the count of
  `reason=scrollback-movement` full redraws must be at most the count of
  offset-*changing* wheel events, not ≈1:1 with all wheel events as in
  `jk-run-533476` (3,442 full redraws / ~3,441 events).
- Shared Debug info tests must prove copy/hover/scroll behavior after horizontal
  and vertical scroll.
- Pane selection tests must prove selection remains visible after mouse-up/copy,
  clears on click/type/new selection, and auto-scrolls when drag-selecting beyond
  the viewport.
- Any remaining local dialog renderer must have a documented structural reason.

## Findings Checklist

### 1. Build Log Overlay Footer Spacing Is Wrong

Status: open.

Observed behavior:

- In the Docker build overlay, the hint row (`↑↓ scroll PgUp/PgDn page Esc close`)
  sits directly above the status/footer row (`Building Docker image... ... jk-run-533476`).
- The expected blank spacer after hints and before the status bar is missing.

Repro steps:

1. Launch PR #495 with `--debug`.
2. Open the Docker build log overlay during image build.
3. Inspect the bottom rows.

Expected behavior:

- Build log body, then the hint row, then a spacer, then the status/footer row.
- Spacing follows the same TUI chrome convention used elsewhere
  (`render_status_footer`,
  `crates/jackin-tui/src/components/status_footer.rs:163`, which places a 1-row
  spacer above the status footer).

Actual behavior:

- Hint row is immediately adjacent to the status/footer row.

Relevant files:

- `crates/jackin-launch/src/tui/components/build_log_dialog.rs`
- `crates/jackin-launch/src/tui/components/footer.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/status_footer.rs`
- `crates/jackin-tui/src/components/hint_bar.rs`

Starting anchors:

- `build_log_box_area` — `build_log_dialog.rs:28`
- `render_build_log_dialog` — hint/footer row math at `build_log_dialog.rs:197-206`
- `render_footer` — `footer.rs:27`
- `render_status_footer` — `status_footer.rs:163`
- `render_hint_bar` — `hint_bar.rs:78`

Evidence (verified):

- `build_log_box_area()` reserves only two rows below the box:
  `area.height.saturating_sub(2)` at `build_log_dialog.rs:28`. The standard
  stack needs three (hint, spacer, footer).
- `render_build_log_dialog()` places `hint_area` at `y + h - 2` and
  `footer_area` at `y + h - 1` (`build_log_dialog.rs:197-206`) — no spacer row.

Suspected root cause:

- Build log overlay manually composes the bottom rows instead of using a shared
  footer/hint stack with the standard spacer policy.

Blocks checklist:

- Blocks Defect 54 visual smoke polish.
- Related to the earlier footer spacing design rule.

Acceptance:

- Build log overlay bottom chrome matches the standard status/hint spacing.
- Add/adjust tests or snapshots that prove the spacer row exists. The existing
  test `build_log_overlay_keeps_status_footer_in_debug_mode`
  (`build_log_dialog.rs:405`) already asserts footer presence — extend it (or
  add a sibling) to assert the spacer row, since it currently passes against
  the broken layout.
- Verify on a real terminal after rebuild.

Close when:

- A focused render test/snapshot proves the hint row and footer/status row are
  separated according to the shared footer stack.
- Real smoke confirms the Docker build overlay no longer places hints directly
  against the status bar.

### 2. Capsule Pane Scrollbar Is Missing Until Scrollback Offset Changes

Status: open.

Observed behavior:

- A pane with content longer than the visible area sometimes shows no scrollbar.
- The scrollbar appears only after scrolling, and even then it is unstable.

Repro steps:

1. Start a real capsule session under `--debug` (rebuild + export
   `JACKIN_CAPSULE_BIN` first — see "Capsule binary resolution trap").
2. Open a pane whose content exceeds available height.
3. Observe whether the right-side scrollbar is present before manually scrolling.
4. Scroll up/down and observe flicker/reactivity.

Expected behavior:

- If pane content is scrollable, the scrollbar is visible and stable.
- Scrollbar visibility is based on overflow, not on whether the pane is currently
  scrolled away from the live tail.

Actual behavior:

- The pane scrollbar is gated on scrollback offset and only appears while
  actively scrolled back.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/session.rs`
- `crates/jackin-capsule/src/tui/render.rs`
- `crates/jackin-tui/src/scroll.rs`
- `crates/jackin-tui/src/components/scrollable_panel.rs`

Starting anchors:

- `apply_pane_scrollbar` — gating condition at `view.rs:322-324`
- `tail_vertical_thumb` — `crates/jackin-tui/src/scroll.rs:439`
- `scrollback_filled` — `crates/jackin-capsule/src/session.rs:705`
- `Session::scroll_by` — `crates/jackin-capsule/src/session.rs:673` (capsule-local;
  shared equivalent is `ScrollState::scroll_by`, `crates/jackin-tui/src/scroll.rs:79`)

Evidence (verified):

- `apply_pane_scrollbar()` uses tail-scroll rendering.
- Current logic gates on `filled > 0 && offset > 0`
  (`crates/jackin-capsule/src/tui/view.rs:322-324`), so a scrollable pane at the
  live tail (`offset == 0`) does not display a scrollbar.

Suspected root cause:

- Capsule pane scrollbars use a local tail-relative special case instead of the
  shared scrollability/overflow helpers.

Blocks checklist:

- Blocks Defect 54 resize/scrollback/zero-ghosting smoke.
- Reopens scrutiny around shared scroll helper adoption.

Acceptance:

- Scrollbar appears whenever pane content is scrollable.
- Scrollbar remains stable at live tail and while scrolled back.
- Scrollbar visibility, hit-testing, wheel/key scroll, and render math use one
  shared helper path or a documented shared adapter.
- Add regression coverage for scrollable-at-tail and scrolled-back states.

Close when:

- Tests cover a scrollable pane at live tail and a pane scrolled back.
- Live smoke confirms the scrollbar is visible before scrolling and remains
  stable after scrolling.
- The implementation uses shared scroll math or a clearly documented adapter over
  shared scroll math.

### 3. Capsule Scrollback Wheel Causes Full-Screen Flicker

Status: open.

Observed behavior:

- Wheel scrolling a capsule pane causes visible full-screen flicker.
- Flicker occurs even when the scroll offset is already saturated and does not
  change.

Repro steps:

1. Start a multi-pane capsule session under `--debug` (rebuild + export
   `JACKIN_CAPSULE_BIN` first).
2. Scroll a pane up/down with the mouse wheel.
3. Continue scrolling after the pane has reached max scrollback.
4. Watch for full-screen flicker.

Expected behavior:

- Wheel events that do not change scroll offset should not redraw.
- Wheel events that do change scroll offset should repaint only the needed region.
- Bottom chrome should not be redrawn through competing paths.

Actual behavior:

- Every wheel event returns a full redraw for `scrollback-movement` (≈1:1 in the
  evidence run: 3,442 full redraws for ~3,441 wheel events).
- Saturated no-op wheel events still full-redraw (multiplexer.log lines
  2476–2480: `before=9 filled=9` → `after=9` → `damage=full`).

Relevant files:

- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/daemon/compositor.rs`
- `crates/jackin-capsule/src/tui/update.rs`
- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/render.rs`

Starting anchors:

- Wheel dispatch + redraw request — `input_dispatch.rs:427-443`:
  `session.scroll_by(delta)` at line 437; line 443 unconditionally returns
  `Some(self.compose_full_redraw(wheel_scrollback_redraw_reason()))` — no
  before/after offset comparison.
- `wheel_scrollback_redraw_reason` — `crates/jackin-capsule/src/tui/update.rs:195`
  (the `ScrollbackMovement` reason variant is in `update.rs:44`).
- `compose_full_redraw` — `compositor.rs:56`.
- `compose_direct_dirty_pane_frame` — `compositor.rs:431` (the existing partial
  path; logs `render: kind=partial reason=pty-output ... via=direct-grid-patch`
  at `compositor.rs:469`). This is the path scrollback movement could reuse.

Evidence (verified):

- Capsule log lines around the wheel repro show (lines 2476–2480):
  - `wheel dispatch: jackin-scrollback ... before=9 filled=9`
  - `wheel dispatch: jackin-scrollback ... after=9`
  - `render: ratatui-frame damage=full ...` followed by both bottom-chrome sites
- Counts over the whole run: 3,442 `kind=full reason=scrollback-movement`
  vs 7,343 `kind=partial` (all partials are `reason=pty-output`; scrollback
  movement never takes the partial path today).
- Resize/render logs show `bottom-chrome: site=ratatui` and
  `bottom-chrome: site=raw-full` strictly paired, 6,850 each.

Suspected root cause:

- Wheel dispatch does not compare before/after offset before requesting redraw
  (`input_dispatch.rs:437-443`).
- Scrollback movement forces `compose_full_redraw()` instead of using the
  partial direct-grid patch path (`compose_direct_dirty_pane_frame`,
  `compositor.rs:431`) when possible.
- Bottom chrome ownership is split between Ratatui and raw append paths (sites
  at `view.rs:267` and `view.rs:41`; a third `site=dialog` at `view.rs:104`).

Blocks checklist:

- Blocks Defect 54 resize/scrollback/zero-ghosting.
- Blocks live performance run-id confidence for bytes-on-wire and present-frame
  behavior.

Acceptance:

- No-op scroll events do not request redraw.
- Offset-changing scroll events avoid whole-screen flicker.
- Logs prove no saturated scrollback event creates a full redraw.
- Add focused tests for no-op wheel dispatch and scrollback redraw reason.
  Existing redraw-reason tests to extend:
  `wheel_redraw_reason_uses_visible_update_vocabulary`
  (`crates/jackin-capsule/src/tui/update/tests.rs:156`) and
  `pane_data_redraw_reason_prioritizes_scrollback_snap` (`tests.rs:100`).
- Validate in a real `--debug` capsule run and record the run id.

Close when:

- A unit test proves saturated wheel scroll does not request a redraw.
- A focused integration/compositor test proves an offset-changing scroll does not
  redraw unrelated chrome unless required.
- A live `--debug` run id includes log evidence for saturated wheel events with no
  full redraw (grep the new multiplexer.log: saturated `before=N ... after=N`
  pairs must not be followed by `kind=full reason=scrollback-movement`).

### 4. Debug Info Dialog Hides Capsule Status Bar

Status: open.

Observed behavior:

- Opening Debug info inside the capsule replaces the whole screen with a blank
  modal backdrop.
- The capsule status bar is not displayed.

Repro steps:

1. Start a capsule session under `--debug` (rebuild + export
   `JACKIN_CAPSULE_BIN` first).
2. Open Debug info from the capsule status/chrome.
3. Observe whether the status bar remains visible.

Expected behavior:

- If the screen had a status bar before opening Debug info, that status bar
  remains visible.
- Debug info is an overlay over the content area, not a full-screen replacement
  that covers status/footer chrome.

Actual behavior:

- The capsule modal path paints a backdrop over `frame.area()` and returns before
  rendering `StatusBarWidget`.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/components/chrome.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`
- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`

Starting anchors:

- `render_capsule_ratatui_frame` — dialog-open early return at `view.rs:234-239`
- `StatusBarWidget` — `crates/jackin-capsule/src/tui/components/chrome.rs:32`
- `DialogBackdrop` — re-export of shared `ModalBackdrop` at `chrome.rs:195`
- `DialogRatatuiSnapshot::DebugInfo` — mapping at `dialog_widgets.rs:251-254`
  (calls `Dialog::container_info_state()`,
  `crates/jackin-capsule/src/tui/components/dialog.rs:399`)
- `render_container_info_on_blank` —
  `crates/jackin-tui/src/components/container_info.rs:410` (backdrop painted
  over `full_area` at line 416)

Evidence (verified):

- `render_capsule_ratatui_frame()` treats `view.dialog_open` as screen-owning
  (`view.rs:234-239`): `frame.render_widget(DialogBackdrop, frame.area())`,
  render dialog, `return` — `StatusBarWidget` is never reached.
- `DialogRatatuiSnapshot::DebugInfo` calls `render_container_info_on_blank()`.
- `render_container_info_on_blank()` paints `ModalBackdrop` over the full area
  (`container_info.rs:416`).
- Note: the backdrop *widget* is already shared (`chrome.rs:195` aliases
  `ModalBackdrop`); the defect is the full-frame area + early return, not a
  duplicate widget.

Suspected root cause:

- The capsule inherited a legacy screen-owning modal model that conflicts with
  the current footer/status design decision.

Blocks checklist:

- Blocks Defect 54 visual smoke.
- Violates `docs/content/docs/reference/tui/dialogs.mdx` status/footer rules
  (see "Binding Design Rules" above).

Acceptance:

- Capsule Debug info preserves status bar/chrome when opened from a screen with
  status bar/chrome.
- Backdrop excludes reserved status/footer rows.
- Tests prove dialog-open capsule frames still render status bar rows.

Close when:

- A capsule render test proves Debug info and status bar are visible in the same
  frame.
- No capsule Debug info path paints `ModalBackdrop` over the full terminal area
  when the status bar is expected.
- Live smoke confirms status bar remains visible under Debug info.

### 5. Debug Info Has Multiple Visual Variants Across Surfaces

Status: open.

Observed behavior:

- Console Debug info, launch Debug info, and capsule Debug info do not look and
  behave like one component.
- Hints, footer behavior, backdrop behavior, horizontal placement, status bar
  preservation, and copy affordances differ.

Observed variants:

1. Host console Debug info: fewer rows, footer/status visible, run chip visible.
2. Capsule Debug info with Codex: container/capsule rows, no status bar, floating
   hint under dialog.
3. Capsule Debug info with Claude: same row family but different placement/hint
   behavior.
4. Launch Debug info during cockpit: blank/fullscreen backdrop differs again.

Expected behavior:

- One shared Debug info dialog across jackin'.
- The only difference between surfaces is which rows are available.

Actual behavior:

- Shared row model exists, but each surface chooses its own shell, backdrop,
  hints, footer, and input wiring.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-console/src/tui/components/container_info.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/components/modal_layout.rs`
- `crates/jackin-launch/src/tui/components/container_info_dialog.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`

Starting anchors:

- `DebugInfo::into_state` — `crates/jackin-tui/src/components/container_info.rs:122-145`
- `debug_run_info_state` — `crates/jackin-console/src/tui/components/container_info.rs:11`
- `launch_container_info_state` — `crates/jackin-launch/src/tui/components/container_info_dialog.rs:14`
- `Dialog::container_info_state` — `crates/jackin-capsule/src/tui/components/dialog.rs:399`
- `render_container_info` — `container_info.rs:384`
- `render_container_info_on_blank` — `container_info.rs:410`

Evidence (verified):

- `DebugInfo` already defines canonical row order and labels
  (`container_info.rs:97-145`).
- Console renders `render_container_info()` into a modal area and relies on
  reserved footer hints.
- Launch and capsule use `render_container_info_on_blank()` and paint a full
  blank backdrop.
- Capsule local `DialogRatatuiSnapshot::DebugInfo` calls the blank renderer
  (`dialog_widgets.rs:251-254`).

Suspected root cause:

- The shared data model was extracted, but the shared dialog shell and modal
  lifecycle contract were not.

Blocks checklist:

- Blocks Defect 54 Debug info and live smoke validation.

Acceptance:

- All Debug info surfaces call one shared top-level render/layout path or a
  shared path with explicit surface options.
- Row order, title, scrollbars, hover, copy, hints, and status/footer behavior
  are consistent.
- Visual regression tests or snapshots cover console, launch, and capsule
  variants.

Close when:

- Console, launch, and capsule tests assert the same canonical row order for the
  rows each surface can provide.
- Render tests or snapshots prove the same title, shell, scrollbar behavior,
  copy affordance, hover state, and footer/status policy.
- Remaining surface-specific code is limited to fact assembly and state storage.

### 6. Debug Info Run ID And Diagnostics Log Must Be Semantically Correct

Status: open.

Observed behavior:

- One console Debug info screenshot showed `Run ID` as a full diagnostics JSONL
  path while `Diagnostics log` showed the same path.

Expected behavior:

- `Run ID` is the bare run id, for example `jk-run-b93735`.
- `Diagnostics log` is the full diagnostics JSONL path.

Actual behavior:

- At least one observed screen displayed the diagnostics path in both rows.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-console/src/tui/components/container_info.rs`
- `crates/jackin/src/console/tui/run.rs`
- `crates/jackin-launch/src/tui/components/container_info_dialog.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`
- `crates/jackin-capsule/src/container_context.rs`

Starting anchors:

- `DebugInfo { run_id, diagnostics_log_path }` — field docs at
  `container_info.rs:110-113` (`run_id` is documented bare;
  `diagnostics_log_path` is the absolute JSONL path)
- `debug_run_info_state` — `crates/jackin-console/src/tui/components/container_info.rs:11`
- `run.run_id()`
- `run.path()`
- `diagnostics.run_id`
- `diagnostics.run_log_display`

Evidence (verified):

- `DebugInfo` comments explicitly state `run_id` is bare and
  `diagnostics_log_path` is absolute path (`container_info.rs:110-113`).
- Console builder currently accepts `run_id` and `log_path` separately; verify
  all call sites pass those values correctly in live/current code.

Suspected root cause:

- A surface or stale path may be passing `run.path()` where `run.run_id()` is
  expected, or an old binary/session may have been used during the screenshot.

Blocks checklist:

- Blocks Debug info trust and copy affordance acceptance.

Acceptance:

- Tests assert `Run ID` row is bare across console, launch, and capsule builders.
- Tests assert `Diagnostics log` row is path/hyperlink/copyable.
- Live smoke confirms row values are correct.

Close when:

- `rg -n 'Run ID.*jsonl|Run ID.*diagnostics'` finds no rendered/test output
  indicating a path in the Run ID row, except negative tests.
- Tests fail if a builder passes a diagnostics path as `run_id`.
- Live Debug info screenshots/logged state show bare Run ID plus full Diagnostics
  log path.

### 7. Dialogs Must Preserve Existing Status/Footer Bars

Status: open.

Observed behavior:

- Debug info hides the capsule status bar.
- Build log overlay owns the full launch screen and composes its own bottom rows.
- Some dialogs render floating hints instead of using reserved footer chrome.

Expected behavior:

- If a screen had a status/footer bar before a dialog opened, the dialog must
  preserve it.
- Dialog hints live in the reserved footer/hint area, not inside or under the
  dialog body.
- Full-screen overlays that intentionally hide the underlying surface must still
  respect the standard footer/hint/status layout.

Actual behavior:

- Capsule and launch blank-backdrop helpers cover full frame areas.
- Console follows the desired model more closely: modal content in modal rect,
  hints in reserved footer.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-launch/src/tui/view.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/view/frame.rs`
- `crates/jackin/src/console/tui/layout/prepare.rs`

Starting anchors:

- `dialog_backdrop` — `crates/jackin-launch/src/tui/components/dialog.rs:13`
- `render_launch_frame`
- `render_capsule_ratatui_frame` — `crates/jackin-capsule/src/tui/view.rs:234-239`
- `prepare_visible_modal` — `crates/jackin/src/console/tui/layout/prepare.rs:43`
  (the console-side model to generalize: subtracts footer height before centering)
- `modal_outer_rect` — `crates/jackin/src/console/tui/components/modal_layout.rs:16`
- `footer_hint_spans` — capsule-local at
  `crates/jackin-capsule/src/tui/components/dialog.rs:1317`

Evidence:

- `docs/content/docs/reference/tui/dialogs.mdx` says status/hint rows are
  inviolable and modal backdrop must not cover footer (verbatim quotes under
  "Binding Design Rules").
- `docs/content/docs/reference/tui/navigation.mdx` says hints are footer-only,
  never internal dialog lines.

Suspected root cause:

- There is no shared modal-layer helper that receives content area and footer
  area separately for all surfaces.

Blocks checklist:

- Blocks Defect 54 visual smoke.

Acceptance:

- Shared dialog layer computes modal rects inside content area, excluding
  reserved footer/status rows.
- Capsule, launch, and console obey the same footer preservation rule.
- Tests prove the footer/status rows remain visible under each relevant dialog.

Close when:

- Every status-preserving modal path receives a content area that excludes
  reserved footer/status rows.
- Backdrop render tests prove reserved rows are not covered.
- Hints for modals are rendered by the reserved footer/hint layer, not as
  floating lines below dialog boxes.

### 8. Context7 API Key Prompt And Other Dialogs Must Use The Same Dialog System

Status: open.

Observed behavior:

- The Context7 API key prompt has its own visual shape and spacing.
- Capsule text-input dialogs and picker dialogs also have local renderer logic.

Orientation note (verified): the string `context7` does not appear anywhere in
the source tree — the prompt the operator saw is an *instance* of the generic
launch text prompt (`draw_text_prompt`,
`crates/jackin-launch/src/tui/components/prompts.rs:46`, driven from
`crates/jackin-launch/src/tui/run.rs:457`) whose label/content comes from role
or MCP configuration data. Fixing the shared prompt shell fixes the Context7
instance; do not hunt for Context7-specific rendering code.

Repro steps:

1. Trigger MCP/Context7 API key setup prompt (use a role whose manifest
   configures an MCP server that needs an API key).
2. Compare it with Debug info, capsule rename prompt, capsule picker, and console
   modal shapes.

Expected behavior:

- All dialogs use the shared dialog system.
- Surface-specific code supplies content/state; shared code supplies shell,
  spacing, backdrop, footer/hints, scroll, and input affordances.

Actual behavior:

- Launch prompt uses shared `render_text_input()` and shared
  `text_input_prompt_rect()` for the one-label prompt box, but still owns
  launch backdrop/footer geometry.
- Capsule previously had a local `render_text_input_dialog()` in
  `dialog_widgets.rs`; implementation evidence now shows this path routed
  through the shared `jackin-tui` labeled text-input helper.
- Capsule filter/info dialogs build their own panels and hint behavior.

Relevant files:

- `crates/jackin-launch/src/tui/components/prompts.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/text_input.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`
- `crates/jackin-tui/src/components/panel.rs`

Starting anchors:

- `draw_text_prompt` — `prompts.rs:46`
- `text_prompt_rect` — `prompts.rs:93`
- `dialog_backdrop` — `crates/jackin-launch/src/tui/components/dialog.rs:13`
- `render_text_input` — `crates/jackin-tui/src/components/text_input.rs:385`
- `text_input_prompt_rect` — `crates/jackin-tui/src/components/text_input.rs`
- `render_labeled_text_input_dialog` —
  `crates/jackin-tui/src/components/text_input.rs`
- `DialogRatatuiSnapshot::TextInputDialog`

Evidence (verified):

- `draw_text_prompt()` calls launch-local `dialog_backdrop()` (`dialog.rs:13`)
  but uses shared `text_input_prompt_rect()` instead of a local text prompt
  rectangle.
- `cargo test -p jackin-tui text_input_prompt_rect --locked` proves the shared
  launch prompt rectangle keeps the current 60%-wide, 5-row prompt shape.
- Capsule `DialogRatatuiSnapshot::TextInputDialog` now routes through
  `jackin_tui::components::render_labeled_text_input_dialog()` instead of a
  private renderer.
- `cargo test -p jackin-tui labeled_text_input_dialog --locked` proves the
  shared shell renders the title, label, value, and cursor styling.
- `cargo test -p jackin-capsule rename_tab --locked` proves the capsule rename
  path still drives the shared renderer-backed dialog state.

Suspected root cause:

- Shared low-level widgets existed, but dialog shell/layout was fragmented. The
  capsule text-input duplicate has been removed; launch prompt geometry/backdrop
  still needs final smoke comparison before closing F8.

Blocks checklist:

- Blocks the operator's reusability requirement for the TUI architecture epic.

Acceptance:

- Context7 prompt, launch text prompts, capsule text input, and console text
  input use a shared text-input dialog path or documented shared adapter.
- Remove or reduce local duplicate renderers where possible.
- Tests/snapshots show consistent shell/spacing/focus/hints.

Scope boundary:

- Required in this goal: fix the Context7/launch prompt path and capsule
  text-input/dialog paths that share the observed inconsistent shell behavior.
- Required audit: enumerate all dialog variants and record any remaining
  surface-local renderer with a structural reason.
- Not required: a broad unrelated rewrite of every picker if the shared
  footer/status/Debug info contract can be completed without it.

Close when:

- A repo-wide search shows no duplicate text-input dialog renderer unless it is
  documented as an adapter over shared primitives.
- The Context7 prompt and capsule text input share shell, spacing, and footer
  behavior with the shared dialog system.

### 9. Debug Info Must Have Full Shared Interaction Contract

Status: open.

Observed behavior:

- Horizontal scroll, hover color, copy behavior, and copy hints exist in pieces
  but are buggy and inconsistent.
- Copy affordances are not consistently visible for all copyable values.
- The evidence run recorded 2,700 `cockpit-dialog-mouse` events with
  `container_info_open=true` (JSONL lines 72–74 onward) — the dialog absorbs a
  mouse-move flood, and on the capsule each hover change composes a full dialog
  overlay redraw.

Expected behavior:

- Debug info displays a horizontal scrollbar whenever any row exceeds available
  width.
- The scroll hint advertises horizontal scroll only when horizontal overflow
  exists.
- Copyable values include Container ID, Run ID, and Diagnostics log whenever
  present.
- Copyable values are hoverable; hovering over the value text changes its color
  slightly and reliably.
- The dialog visibly tells the operator how to copy each value.
- Click-to-copy works on copyable values.
- Keyboard copy works consistently according to the shared footer/hint contract.
- OSC 8 hyperlink overlay for diagnostics log follows the visible horizontally
  scrolled slice.

Copy affordance requirement:

- Do not satisfy this with invisible click targets alone. Render an explicit
  shared copy affordance for copyable rows. The footer must also explain the copy
  action available on the current surface.

Actual behavior:

- Existing implementation has shared pieces, but each surface wires mouse move,
  copy, scroll, and hint state separately.
- Capsule hover redraw currently composes a full dialog overlay redraw.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-launch/src/tui/subscriptions.rs`
- `crates/jackin/src/console/tui/input/mouse.rs`
- `crates/jackin/src/console/effects.rs`
- `crates/jackin-capsule/src/daemon/mouse_input.rs`
- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`

Starting anchors:

- `ContainerInfoState` — `container_info.rs:154-164`
- `ContainerInfoRow::copyable` — builder at `container_info.rs:54`,
  `is_copyable` at line 76
- `mark_copied` — `container_info.rs:289`
- `set_hovered_row` — `container_info.rs:301`
- `copy_payload_at` — `container_info.rs:421`
- `value_placements` — `container_info.rs:481`
- `hyperlink_overlay` — `container_info.rs:439`
- `debug_info_hint_spans` — `container_info.rs:335`

Evidence (verified):

- `ContainerInfoState` already stores `DialogBodyScroll`, copied row, and
  hovered row (`container_info.rs:154-164`).
- `copy_payload_at()` and `value_placements()` already account for scroll axes.
- `hyperlink_overlay()` accounts for the visible horizontally scrolled slice.
- Console, launch, and capsule each have separate hover/copy routing.

Suspected root cause:

- Interaction behavior is not centralized with the renderer contract.
- Footer hint vocabulary does not yet express per-row copy affordances clearly
  enough for all surfaces.

Blocks checklist:

- Blocks Debug info smoke acceptance.
- Blocks operator confidence in diagnostics collection.

Acceptance:

- Shared tests cover horizontal overflow, horizontal scrollbar visibility,
  horizontal hint gating, hover hit-test after horizontal/vertical scroll, copy
  hit-test after scroll, copied-row feedback, and diagnostics hyperlink overlay.
- Console, launch, and capsule tests prove they all preserve and update
  hover/copy/scroll state correctly.
- Live smoke confirms all copyable values can be copied and hovered.

Close when:

- The explicit copy affordance is visible for Container ID, Run ID, and
  Diagnostics log whenever those rows exist.
- Hover state changes the visible value/affordance color reliably and only on
  copyable cells.
- Mouse click and keyboard copy paths copy the same payload for the same row.
- Tests cover copy/hover after horizontal scroll and after vertical scroll.
- Live smoke confirms copy works for Container ID, Run ID, and Diagnostics log.

### 10. Scrollable Pane Text Selection Must Persist, Copy, And Auto-scroll

Status: open.

Observed behavior:

- In a capsule pane, text selection is copyable and copies to the clipboard on
  mouse-up.
- The visible selection disappears immediately after the operator finishes
  selecting.
- There is no visible confirmation that the selected text was copied.
- Drag-selecting beyond the top or bottom of the visible scrollable pane does not
  auto-scroll the pane to extend the selection.

Repro steps:

1. Start a capsule session with enough pane content to overflow vertically
   (rebuild + export `JACKIN_CAPSULE_BIN` first).
2. Drag-select text inside the pane.
3. Release the mouse button.
4. Observe whether the selection remains highlighted and whether copied feedback
   appears.
5. Start selecting near the bottom and drag below the visible pane edge.
6. Start selecting near the top and drag above the visible pane edge.

Expected behavior:

- The pane selection model is read-only: select + copy only, with no edit/delete
  semantics.
- Selection remains highlighted after mouse-up.
- Mouse-up copies selection to clipboard and shows a visible `Selection copied`
  style confirmation in a transient overlay/toast.
- Copy-success feedback must not use the hint/footer row. That row is reserved
  for currently available actions in the focused surface.
- Clicking unrelated content/chrome or starting to type clears the persisted
  selection.
- Starting a new selection replaces the previous selection.
- Dragging beyond the top/bottom edge auto-scrolls the scrollable pane and
  extends the selection in the drag direction.
- Auto-scroll stops at content bounds and does not flicker or full-redraw the
  whole screen unnecessarily.

Actual behavior:

- Selection highlight only exists during active drag selection.
- The selected area disappears after mouse-up.
- Copy success is silent.
- Selection does not auto-scroll beyond the visible area.

Relevant files:

- `crates/jackin-capsule/src/daemon/mouse_input.rs`
- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/daemon/compositor.rs`
- `crates/jackin-capsule/src/session.rs`
- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/render.rs`
- `crates/jackin-tui/src/scroll.rs`

Starting anchors:

- `Selection`
- `select`
- `clipboard`
- `copy`
- `mouse drag`
- `MouseEventKind::Drag`
- `MouseEventKind::Up`
- `scrollback_offset`
- `Session::scroll_by` — `crates/jackin-capsule/src/session.rs:673`
- `scrollback_filled` — `crates/jackin-capsule/src/session.rs:705`

Evidence:

- Operator observed correct clipboard copying but no persistent selection or
  visible copied feedback.
- Operator observed that drag selection does not auto-scroll when selecting past
  the visible viewport.

Suspected root cause:

- The current selection path treats selection as an active drag overlay only, not
  as a persistent content-coordinate range.
- Copy feedback is not connected to a transient overlay/toast that can appear
  without replacing the focused surface's action hints.
- Drag selection does not own an edge-auto-scroll ticker/path.

Blocks checklist:

- Blocks Defect 54 live smoke polish.
- Blocks expected behavior for scrollable pane selection.

Acceptance:

- Selection supports copy only; it never edits, deletes, cuts, replaces, or
  pastes pane content.
- Selection range is stored in content coordinates with anchor/focus semantics.
- Mouse-up copies selected text and leaves selection visibly highlighted.
- A visible copied confirmation appears in a transient overlay/toast outside the
  hint/footer row.
- Clicking unrelated content/chrome clears the selection.
- Typing clears the selection before forwarding the key to the pane.
- Starting a new selection replaces the old selection.
- Wheel scrolling after selection keeps the same content range selected when it
  remains visible.
- Dragging beyond top/bottom viewport edges auto-scrolls and extends selection.
- Auto-scroll is bounded, stable, and does not send drag-scroll input to the PTY.

Close when:

- Tests cover mouse-up copy with persisted selection.
- Tests cover clear-on-click, clear-on-type, and replace-on-new-selection.
- Tests cover selection rendering after scroll offset changes.
- Tests cover upward and downward auto-scroll while drag-selecting beyond the
  pane viewport.
- Live smoke confirms persistent highlight, copied feedback, deselect behavior,
  and edge auto-scroll in a real scrollable capsule pane.

## Existing Tests To Extend Or Expect To Break

Inventory of the tests already guarding this territory. Extend these rather
than writing parallel siblings, and expect the flagged ones to need updating
when the contract changes:

| Test | Location | Role in this goal |
| --- | --- | --- |
| `renders_rows_with_title_and_link_style` | `crates/jackin-tui/src/components/container_info/tests.rs:12` | Baseline shared-render assertion; extend for shell contract. |
| `copy_payload_at_hits_copyable_value_column` | `container_info/tests.rs:35` | Extend with post-scroll hit-tests (both axes). |
| `hyperlink_overlay_emits_osc8_for_link_rows` | `container_info/tests.rs:60` | Extend with horizontally scrolled slice. |
| `long_value_shows_horizontal_scrollbar_and_scroll_reveals_tail` | `container_info/tests.rs:78` | Already covers h-overflow; add hint-gating assertion. |
| `short_content_shows_no_horizontal_scrollbar` | `container_info/tests.rs:119` | Negative case; keep. |
| `blank_render_clears_full_background_to_terminal_default` | `container_info/tests.rs:137` | **Will break/change** when `render_container_info_on_blank` is retired or re-scoped to a content-only area (Phase 2). Update deliberately, do not band-aid. |
| `wheel_redraw_reason_uses_visible_update_vocabulary` | `crates/jackin-capsule/src/tui/update/tests.rs:156` | Extend for no-op suppression (Finding 3). |
| `pane_data_redraw_reason_prioritizes_scrollback_snap` | `crates/jackin-capsule/src/tui/update/tests.rs:100` | Related redraw-reason coverage. |
| `scrollbar_hit_maps_track_to_top_offset` | `crates/jackin-launch/src/tui/components/build_log_dialog.rs:357` | Build-log scroll hit-testing; keep green through Finding 1. |
| `build_log_overlay_keeps_status_footer_in_debug_mode` | `build_log_dialog.rs:405` | **Currently passes against the broken layout** — extend to assert the spacer row (Finding 1). |

## Ordered Fix Plan

Do not start implementation until the operator says issue collection is complete
or explicitly asks to fix the current set.

### Phase 1 - Lock The Shared Debug Info Contract

- Define the shared Debug info contract in `jackin-tui`:
  - canonical rows and labels;
  - copyable rows;
  - hover state;
  - copied-row state;
  - both-axis scroll state;
  - scroll axes/hints derived from actual overflow;
  - explicit copy affordance rendering;
  - status/footer preservation expectations.
- Add focused unit/snapshot tests in `jackin-tui` for:
  - bare Run ID vs Diagnostics log path;
  - Container ID, Run ID, Diagnostics log copy targets;
  - horizontal overflow and scrollbar;
  - horizontal hint appears only on horizontal overflow;
  - hover color changes only on copyable value cells;
  - copy hit-test follows both scroll axes;
  - copy affordance placement follows both scroll axes;
  - hyperlink overlay follows both scroll axes.

### Phase 2 - Replace Per-Surface Debug Info Shells

- Route console, launch, and capsule Debug info through the same shared
  render/layout contract.
- Remove or deprecate `render_container_info_on_blank()` if it cannot preserve
  status/footer rows. If a blank backdrop is still needed, make it accept a
  content-only area and never cover reserved chrome. (Updating
  `blank_render_clears_full_background_to_terminal_default` is part of this
  step, not collateral damage.)
- Make launch and capsule compute modal rects against the content area, not the
  full terminal. The console-side model to generalize is
  `prepare_visible_modal` (`crates/jackin/src/console/tui/layout/prepare.rs:43`).
- Keep only surface-specific data assembly and state storage outside
  `jackin-tui`.

### Phase 3 - Footer And Hint Unification

- Move Debug info hints into the reserved footer/status area on all surfaces.
- Ensure hints use `HintSpan` (`crates/jackin-tui/src/geometry.rs:32`) and the
  shared hint renderer (`render_hint_bar`, `hint_bar.rs:78`).
- Add copy hints that clearly identify copyable values.
- Render a shared explicit copy affordance for each copyable Debug info row.
- Ensure scroll hints are generated by `scroll_hint_spans()`
  (`dialog_layout.rs:284`) from actual overflow axes.
- Fix build-log overlay spacing through the same footer/hint layout primitive
  (`render_status_footer`, `status_footer.rs:163`).
- Fold the duplicate footer renderers
  (`crates/jackin-console/src/tui/view.rs:71`,
  `crates/jackin-launch/src/tui/components/footer.rs:27`) and the capsule-local
  `footer_hint_spans` (`dialog.rs:1317`) into or onto the shared stack where
  practical; document any that must stay local.

### Phase 4 - Dialog System Reuse Beyond Debug Info

- Audit launch prompts, Context7 API key prompt (generic `draw_text_prompt`
  instance — see Finding 8 orientation note), capsule text input, capsule
  pickers, console modals, and info dialogs.
- Replace duplicate local shell/backdrop/text-input renderers with shared
  `jackin-tui` helpers where practical.
- If a renderer must remain surface-local, document the structural reason and
  keep row spacing, footer/hints, and status preservation identical.

### Phase 5 - Capsule Scrollbar And Flicker Fix

- Change pane scrollbar visibility from `offset > 0` to real scrollability
  (`apply_pane_scrollbar`, `view.rs:322-324`).
- Route pane scrollbar math through shared scroll helpers
  (`crates/jackin-tui/src/scroll.rs`) or a documented shared adapter.
- In wheel dispatch, compare scrollback offset before/after and skip redraw when
  unchanged (`input_dispatch.rs:427-443` currently returns
  `compose_full_redraw(...)` unconditionally at line 443).
- Prefer partial/redraw-minimal frame composition for scrollback movement where
  technically possible (`compose_direct_dirty_pane_frame`, `compositor.rs:431`,
  is the existing partial path; today it serves only `reason=pty-output`).
- Eliminate or explain the dual bottom-chrome draw path (`site=ratatui` at
  `view.rs:267` plus `site=raw-full` at `view.rs:41`; `site=dialog` at
  `view.rs:104`) so chrome does not flicker.

### Phase 6 - Scrollable Pane Selection

- Introduce or extend a pane selection model that stores anchor/focus in content
  coordinates.
- Keep selection visible after mouse-up and copy.
- Add a copied feedback path through a transient capsule overlay/toast, not the
  hint/footer row.
- Clear selection on explicit deselect, typing, pane close/clear, or new
  selection.
- Add edge auto-scroll during active drag selection.
- Ensure selection auto-scroll and scrollback wheel use the same scroll bounds
  (`Session::scroll_by` / `scrollback_filled`,
  `crates/jackin-capsule/src/session.rs:673/705`).

### Phase 7 - Docs

- Update `docs/content/docs/reference/tui/dialogs.mdx` with the Debug info
  component contract after code and tests pass. Include row order, copyable rows,
  explicit copy affordance, hover behavior, horizontal scroll, and footer/status
  preservation.
- Update `docs/content/docs/reference/tui/components.mdx` if the reusable
  component catalog changes. Add the shared Debug info/dialog shell if it becomes
  a named component.
- Update `docs/content/docs/reference/tui/navigation.mdx` only if hint/copy/hover
  rules need more explicit wording. Keep scroll-hint rules there.
- Add the scrollable pane text-selection contract to the TUI docs. The best home
  is `docs/content/docs/reference/tui/navigation.mdx` because this is input,
  selection, scroll, and visible feedback behavior.
- Do not mark roadmap/checklist items complete until the required command output
  or run ids exist.

### Phase 8 - Verification

Focused tests per finding first (fast inner loop):

```sh
cargo nextest run -p jackin-tui -E 'test(/container_info/)'
cargo nextest run -p jackin-capsule -E 'test(/update::tests/)'
cargo nextest run -p jackin-launch -E 'test(/build_log/)'
```

Then broad gates:

```sh
cargo fmt --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked -E 'not binary(dind_e2e)'
```

(The `-E 'not binary(dind_e2e)'` filter is load-bearing locally: `dind_e2e` is
gated behind the `e2e` feature, which `--all-features` turns on, and the suite
needs Docker plus an exported `JACKIN_CAPSULE_BIN`. CI runs it with the capsule
binary prebuilt; locally run it deliberately, not by accident.)

Run docs gates if TUI docs are edited:

```sh
cd docs
bun run build
bun run check:repo-links
bunx tsc --noEmit
bun test
```

Run a real operator smoke after rebuilding (capsule binary freshly exported —
see "Capsule binary resolution trap"):

- Capsule smoke build and real multi-pane `--debug` session.
- Debug info on console, launch, and capsule.
- Horizontal scroll in Debug info with a long diagnostics path.
- Hover and copy Container ID, Run ID, and Diagnostics log.
- Build log overlay footer spacing.
- Pane scrollbar visible at live tail and while scrolled back.
- Wheel scroll no-op does not flicker or full redraw.
- Pane text selection remains visible after copy.
- Pane copied feedback appears as a transient overlay/toast outside the
  hint/footer row.
- Clicking elsewhere and typing clear the persisted selection.
- Drag-selecting past the top/bottom edge auto-scrolls and extends selection.

Capture run ids and relevant log lines before marking any Defect 54 checklist
item complete. Use the template below.

## Evidence Capture Template

One block per smoke run, appended to this file (or the Defect 54 ledger) before
any checklist box flips to `[x]`:

```text
### Smoke run: <date>
- Command: <exact cargo run invocation incl. --debug and env exports>
- Capsule binary: <JACKIN_CAPSULE_BIN path or "release/cache (unchanged)">
- Run id: <jk-run-XXXXXX>
- JSONL: <path>
- Capsule log: <path from container_started detail.capsule_log>
- Findings exercised: <F1..F10 list>
- Key greps + results:
  - <grep command> -> <count / line numbers / excerpt>
- Verdict per finding: <Fn: pass/fail + one-line observation>
```

## Traceability Matrix

| Finding | Phases | Defect / checklist tie-in | Key tests |
| --- | --- | --- | --- |
| F1 build-log spacing | 3 | Defect 54 visual smoke polish | `build_log_overlay_keeps_status_footer_in_debug_mode` (+ spacer assertion) |
| F2 pane scrollbar gating | 5 | Defect 54 resize/scrollback/zero-ghosting | new scrollable-at-tail + scrolled-back tests |
| F3 wheel full-redraw flicker | 5 | Defect 54 zero-ghosting; live perf run-id confidence; adjacent to Defect 58's re-run of the Defect 44 manual repro | `wheel_redraw_reason_uses_visible_update_vocabulary` (+ no-op suppression) |
| F4 capsule status bar hidden | 1, 2 | Defect 54 visual smoke; dialogs.mdx rules | new capsule frame test (dialog + status bar same frame) |
| F5 Debug info variants | 1, 2, 3 | Defect 54 Debug info validation | per-surface row-order/shell tests |
| F6 Run ID semantics | 1 | Debug info trust | builder tests across console/launch/capsule |
| F7 status/footer preservation | 2, 3 | Defect 54 visual smoke | backdrop-excludes-reserved-rows tests |
| F8 dialog system reuse | 4 | TUI architecture epic reusability | text-input shell consistency tests |
| F9 interaction contract | 1, 3 | Debug info smoke acceptance | extended `container_info/tests.rs` suite |
| F10 pane text selection | 6 | Defect 54 live smoke polish | selection persist/clear/auto-scroll tests |

## Glossary

- **Render kinds** — `render: kind=full` (whole-screen recomposition via
  `compose_full_redraw`, `compositor.rs:56`) vs `kind=partial` (dirty-pane
  patches; `via=direct-grid-patch` marks the minimal-grid path from
  `compose_direct_dirty_pane_frame`, `compositor.rs:431/469`).
- **`FullRedrawReason`** — vocabulary for why a full redraw was requested;
  `scrollback-movement` comes from `wheel_scrollback_redraw_reason()`
  (`update.rs:195`, variant at `update.rs:44`).
- **Bottom-chrome sites** — debug tags for which code path drew the bottom
  chrome: `site=ratatui` (`view.rs:267`), `site=raw-full` (`view.rs:41`),
  `site=dialog` (`view.rs:104`). Two sites firing in the same frame family =
  duplicated chrome work.
- **Frame family** — the burst of log lines belonging to one composed frame
  (render line + its chrome lines, same timestamp neighborhood).
- **Scrollback `offset` / `filled`** — `offset` = how far the pane is scrolled
  away from the live tail (0 = at tail); `filled` = how many scrollback rows
  exist (`session.rs:705`). `before=9 filled=9` then `after=9` = saturated
  no-op wheel event.
- **Content coordinates (selection)** — positions anchored to the pane's
  scrollback content, not the visible screen; a selection stored this way
  survives scrolling and redraws (Finding 10 contract).
- **`DialogRatatuiSnapshot`** — capsule-side enum mapping open-dialog state to
  a Ratatui render call (`dialog_widgets.rs:251-254` for DebugInfo).
- **`cockpit-dialog-mouse`** — JSONL debug event recorded for every mouse event
  while a launch-cockpit dialog is open; fields include
  `container_info_open=`/`build_log_open=`.

## External References Used For Selection Rules

- W3C Selection API: <https://www.w3.org/TR/selection-api/>
- W3C Selection API user interactions: <https://www.w3.org/TR/selection-api/#user-interactions>
- MDN Selection API: <https://developer.mozilla.org/en-US/docs/Web/API/Selection_API>
- MDN Selection object: <https://developer.mozilla.org/en-US/docs/Web/API/Selection>
- W3C Pointer Events `mousedown`: <https://www.w3.org/TR/pointerevents/#the-mousedown-event>
- W3C WAI G149 highlighting technique: <https://w3c.github.io/wcag/techniques/general/G149>

## Do Not Mark Done Yet

These stay open until real evidence exists:

- Defect 54 live smoke ledger — all seven command-ledger items (capsule
  multi-pane resize storm, provider picker, auth source-folder override ×2 run
  ids, symbolicated debug-capsule build, clean-exit/re-attach cycle,
  host-console resize sweep, Docker-capable `dind_e2e` run).
- Defect 58 — the regression tests and buffer-diff documentation are `[x]`;
  the only open piece is re-running the Defect 44 manual resize/ghosting repro
  inside the Defect 54 smoke session and capturing the run id.
- Defect 59 B.5 — source-folder end-to-end smoke (B.3 UI shipped `[x]`; the
  smoke + `auth-sync-source-folder.mdx` update remain).
- Defect 60 — final roadmap sweep (all other items `[x]`; the sweep waits on
  Defects 48–59 closing).
- Defect 63 — deferred license ruling: operator decision pending on
  `adler2@2.0.1` (`0BSD`), `aho-corasick@1.1.4` (`Unlicense`),
  `aws-lc-rs@1.17.0` (`ISC`).
