# Plan 001: Make the TUI component catalog, lookbook, and module docs match reality

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- docs/content/docs/reference/tui/ crates/jackin-tui-lookbook/ crates/jackin-capsule/src/tui/components/palette.rs crates/jackin-tui/src/components/diff_view.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

`docs/content/docs/reference/tui/components.mdx` is the canonical "check this table before writing any new TUI widget" lookup, and the lookbook is the visual-regression surface the docs promise for every shared component. Today the catalog table omits roughly 12 shared `jackin-tui` widgets, contains a "Panel rain" row with no module path that actually describes a launch-local animation, the shared `diff_view` component has no lookbook story at all, the existing `container-info` story has no docs page, one lookbook story hand-rolls a hint line that contradicts the hint conventions it should showcase, and the capsule's `palette.rs` docstring describes a color module while the file contains the command palette. Every one of these actively misleads a contributor following the documented "search the catalog first" workflow, which is how duplicate widgets get written. This plan is pure truth-restoration: docs, stories, and one docstring — no production behavior changes.

## Current state

Relevant files:

- `docs/content/docs/reference/tui/components.mdx` — canonical catalog. The table at lines 47–63 has 15 rows. Shared-crate (`crates/jackin-tui`) rows are only: Text input box, Confirm dialog, Save/discard strip, Toast, Error popup. The last row (line 63) is:
  ```
  | Panel rain | Background rain animation | Background decoration |
  ```
  — note it has only 3 cells (no Module column content) and refers to the launch-local rain animation in `crates/jackin-launch-tui/src/tui/components/rain.rs`.
- `crates/jackin-tui/src/components.rs` — module list of the shared crate (the source of truth for what belongs in the table): `bottom_chrome`, `brand_header`, `button_strip`, `confirm_dialog`, `container_info`, `dialog_layout`, `diff_view`, `error_dialog`, `filter_input`, `focus_owner`, `hint_bar`, `hover_tracker`, `modal_backdrop`, `modal_lifecycle`, `panel`, `save_discard_dialog`, `scrollable_panel`, `select_list`, `status_footer`, `status_popup`, `tab_strip`, `text_input`, `toast`.
- `crates/jackin-tui/src/components/diff_view.rs` — 354-line shared component (`DiffViewState`, `render_diff_view`, side-by-side and single-pane modes) used in production by `crates/jackin-launch-tui/src/tui/run.rs` (worktree-inspect view). It has **no** lookbook story and no docs page.
- `crates/jackin-tui-lookbook/src/stories.rs` — `stories()` at line 95 registers 26 stories via `Story::new(...)`. A `container-info/debug` story exists (registered around lines 259–267). At lines 587–589 the toast story hand-rolls a fake footer:
  ```rust
  // crates/jackin-tui-lookbook/src/stories.rs:588
  Line::from("Ctrl+\\ menu   click focus pane"),
  Line::from("PR #495 · refactor: finish TUI architecture epic"),
  ```
  This bypasses the shared `HintSpan` vocabulary and spells the chord `Ctrl+\` with a `+` join while the real capsule uses `Ctrl-` (see `format_key_glyph` in `crates/jackin-capsule/src/tui/components/dialog/hint.rs`).
- `docs/content/docs/reference/tui/lookbook/index.mdx` — Stories list (lines 34–48) has 15 entries; it omits the existing ContainerInfo story and (of course) DiffView. The coverage rule at lines 51–54 reads: "Every new shared `jackin-tui` component must ship with both lookbook surfaces… A component without both previews is not catalogued."
- `docs/content/docs/reference/tui/lookbook/` — has 15 component pages; there is **no** `container-info.mdx` and no `diff-view.mdx`.
- `crates/jackin-capsule/src/tui/components/palette.rs` — lines 1–9 docstring says "Named color palette for the capsule TUI… capsule components must source colors from `jackin_tui` palette constants." The file actually defines `PaletteCloseLabel`, `PaletteCommand`, `PALETTE_ITEMS` — the Ctrl+J command palette, not colors.

Repo conventions that apply:

- Docs live beside code and must be updated in the same PR as code (`PROJECT_STRUCTURE.md`, "Code ↔ docs cross-reference").
- Lookbook SVG previews are generated artifacts under `docs/public/tui-lookbook`; regenerate with the command below and verify with `--check` (documented in `crates/jackin-tui-lookbook/CLAUDE.md:47-48`).
- Brand spelling: `jackin❯` in rich-text prose; literal `jackin` in code/paths (`RULES.md`).
- Comments state non-obvious WHY only (`ENGINEERING.md`).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format check | `cargo fmt --check` | exit 0 |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests (lookbook) | `cargo nextest run -p jackin-tui-lookbook` | all pass |
| Tests (shared crate) | `cargo nextest run -p jackin-tui` | all pass |
| Regenerate lookbook SVGs | `cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` | exit 0, SVGs written |
| Verify no SVG drift | `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` | exit 0 |
| Docs build | `cd docs && bun run build` | exit 0 |

Never use `cargo test`; this repo mandates `cargo nextest run` (TESTING.md).

## Scope

**In scope** (the only files you should modify/create):
- `docs/content/docs/reference/tui/components.mdx` (catalog table)
- `docs/content/docs/reference/tui/lookbook/index.mdx` (stories list)
- `docs/content/docs/reference/tui/lookbook/container-info.mdx` (create)
- `docs/content/docs/reference/tui/lookbook/diff-view.mdx` (create)
- `docs/content/docs/reference/tui/lookbook/meta.json` (add the two new pages if sibling pages are listed there)
- `crates/jackin-tui-lookbook/src/stories.rs` (add diff-view stories; fix the toast story footer lines)
- `crates/jackin-tui-lookbook/src/tests.rs` (only if a story-count/name test needs the new entries)
- `docs/public/tui-lookbook/` (regenerated SVGs only, via the regen command)
- `crates/jackin-capsule/src/tui/components/palette.rs` (docstring only)

**Out of scope** (do NOT touch, even though they look related):
- Any production render code in `crates/jackin-tui/src/` or `crates/jackin-capsule/src/` (this plan changes docs, stories, and one comment — zero behavior).
- `crates/jackin-console/src/tui/components/agent_choice.rs` — its shape mismatch with the catalog's "two-button family" prose is handled by a separate decision; here you only ensure its catalog row's wording matches what the code does today (a vertical picker over a variable-size choice set, `AgentChoiceState { choices: Vec<A> }`).
- `crates/jackin-launch-tui/` — the rain animation stays where it is; you only fix how the catalog refers to it.

## Git workflow

- This repo forbids committing to `main`. Branch first: `docs/tui-catalog-lookbook-truth` (repo prefixes: `feature/`, `fix/`, `refactor/`, `chore/`; use `docs/` if the operator's tooling accepts it, else `chore/tui-catalog-lookbook-truth`). Propose the branch to the operator and wait for confirmation before creating it (CLAUDE.md "Stay on active branch").
- Conventional Commits with DCO sign-off, push after every commit: `git commit -s -m "docs(tui): align component catalog and lookbook with shared crate" && git push`.
- Do NOT open a PR unless the operator instructed it.

## Steps

### Step 1: Rewrite the catalog table in `components.mdx`

In `docs/content/docs/reference/tui/components.mdx` (table at lines 47–63):

1. Keep all existing correct rows (op picker, role picker, text input, scope/source pickers, agent choice, mount destination choice, workdir picker, confirm dialog, save/discard strip, toast, file browser, GitHub picker, error popup).
2. Fix the "Agent choice" row's "What it provides" cell to describe reality: a vertical picker over a variable-size agent set (see `crates/jackin-console/src/tui/components/agent_choice.rs:29-30`, `pub choices: Vec<A>`), not a "two-button" strip. Also remove `agent_choice.rs` from the "same visual shape: two side-by-side buttons" sentence at line ~119 of the same file (that prose currently lists `scope_picker.rs`, `source_picker.rs`, `agent_choice.rs`, `mount_dst_choice.rs`, `confirm.rs`) — leave the other four.
3. Delete the malformed "Panel rain" row (line 63). If you want to preserve the pointer, add a one-sentence note *below* the table stating that surface-local decorative widgets (e.g. the launch rain in `crates/jackin-launch-tui/src/tui/components/rain.rs`) are intentionally not catalogued as shared components.
4. Add one row per shared visual component currently missing from the table, each with its `<RepoFile path="crates/jackin-tui/src/components/<mod>.rs">` module link, a one-line "What it provides", and a one-line "When to use". Missing modules to add: `brand_header`, `button_strip`, `tab_strip`, `select_list`, `scrollable_panel`, `status_footer`, `hint_bar`, `filter_input`, `panel`, `status_popup`, `container_info`, `diff_view`. (Do not add pure-logic helpers `focus_owner`, `hover_tracker`, `modal_lifecycle`, `dialog_layout`, `modal_backdrop`, `bottom_chrome` as *catalog* rows unless the table already has precedent for non-visual entries — instead list them in one short "Layout & interaction primitives" sentence under the table naming all six with RepoFile links.)

Base each "What it provides" description on the module's own doc comment — open each file and paraphrase its header; do not invent capabilities.

**Verify**: `grep -c '^| ' docs/content/docs/reference/tui/components.mdx` → row count grew by ≥11 vs the current 16 (header + 15 rows); `grep -n 'Panel rain' docs/content/docs/reference/tui/components.mdx` → no table-row match (at most the prose note).

### Step 2: Add diff-view lookbook stories

In `crates/jackin-tui-lookbook/src/stories.rs`, register two new stories in `stories()` following the structure of any existing entry (e.g. the `container-info/debug` story around lines 259–267):

- `diff-view/side-by-side` — build a `DiffViewState` via its side-by-side constructor with 3–5 sample lines including one added and one removed line.
- `diff-view/single-pane` — the single-pane mode (`SinglePaneKind`) variant.

Open `crates/jackin-tui/src/components/diff_view.rs` first and use its actual public constructors (`DiffViewState::side_by_side` / `single_pane` or whatever the real names are — read the file; if the constructors differ from these names, use the real ones). Render via `render_diff_view`. Use static sample content only (the lookbook forbids nondeterminism).

**Verify**: `cargo nextest run -p jackin-tui-lookbook` → pass; `cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` → exit 0 and new `diff-view*.svg` files appear under `docs/public/tui-lookbook/`.

### Step 3: Fix the toast story's hand-rolled footer

In `crates/jackin-tui-lookbook/src/stories.rs` (lines ~587–589), replace the raw strings

```rust
Line::from("Ctrl+\\ menu   click focus pane"),
```

with a line built from real `jackin_tui::HintSpan` values rendered through the shared hint renderer (`jackin_tui::components::hint_bar` — use its public `line()`/`HintBar` API; read `crates/jackin-tui/src/components/hint_bar.rs` for the exact function). The chord glyph must come from the same convention production uses (`Ctrl-\` with hyphen join, matching `format_key_glyph` in `crates/jackin-capsule/src/tui/components/dialog/hint.rs:18`). The second line ("PR #495 · …") is branch-context content, not hints — it may stay a plain string.

**Verify**: `grep -n 'Ctrl+' crates/jackin-tui-lookbook/src/stories.rs` → no matches; regen + `--check` command → exit 0.

### Step 4: Add the two missing lookbook docs pages and index entries

1. Create `docs/content/docs/reference/tui/lookbook/container-info.mdx` and `docs/content/docs/reference/tui/lookbook/diff-view.mdx`, modeled exactly on an existing sibling (open `docs/content/docs/reference/tui/lookbook/toast.mdx` as the structural pattern: frontmatter title/description, short prose, embedded generated SVG(s)).
2. Add `[ContainerInfo]` and `[DiffView]` bullets to the Stories list in `docs/content/docs/reference/tui/lookbook/index.mdx` (lines 34–48), preserving alphabetical-or-existing ordering convention (match whatever ordering the list currently uses — it is insertion-ordered by component; append consistently).
3. If `docs/content/docs/reference/tui/lookbook/meta.json` enumerates pages, add the two new slugs.

**Verify**: `cd docs && bun run build` → exit 0.

### Step 5: Fix the capsule `palette.rs` docstring

Replace the module doc at `crates/jackin-capsule/src/tui/components/palette.rs:1-9` with one describing what the file contains: the Ctrl+J command-palette command set (`PaletteCommand`, `PALETTE_ITEMS`, filter helpers). Preserve the color-sourcing invariant by moving that sentence to where it belongs: the module doc of `crates/jackin-capsule/src/tui/components.rs` (the components module root), stated once: capsule components must source colors from `jackin_tui` theme constants; no inline RGB literals in render code.

**Verify**: `head -5 crates/jackin-capsule/src/tui/components/palette.rs` → mentions command palette, not colors; `cargo nextest run -p jackin-capsule` → pass.

### Step 6: Full verification sweep

Run, in order: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest run -p jackin-tui -p jackin-tui-lookbook -p jackin-capsule`, `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook`, `cd docs && bun run build`. All must exit 0.

## Test plan

- No new unit tests beyond the story registrations (stories are themselves the regression artifacts). If `crates/jackin-tui-lookbook/src/tests.rs` asserts a story count or story-name list, update it to include the two diff-view stories — that update *is* the test.
- Verification: `cargo nextest run -p jackin-tui-lookbook` → all pass including any updated count test; `--check` regen → no drift.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo fmt --check` exits 0
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` exits 0
- [ ] `cargo nextest run -p jackin-tui -p jackin-tui-lookbook -p jackin-capsule` exits 0
- [ ] `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` exits 0
- [ ] `cd docs && bun run build` exits 0
- [ ] `grep -n 'Panel rain' docs/content/docs/reference/tui/components.mdx` — no table row
- [ ] Catalog table contains rows for `select_list`, `scrollable_panel`, `status_footer`, `hint_bar`, `diff_view`, `container_info`, `tab_strip`, `button_strip`, `brand_header`, `filter_input`, `panel`, `status_popup`
- [ ] `docs/content/docs/reference/tui/lookbook/diff-view.mdx` and `container-info.mdx` exist and are linked from `index.mdx`
- [ ] `grep -rn 'Ctrl+' crates/jackin-tui-lookbook/src/stories.rs` returns nothing
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The catalog table or lookbook index has materially changed since `a2ec1b237` (drift check hits).
- `diff_view.rs` has no public constructors suitable for a static story (you would have to add public API to `jackin-tui` — that is production-code scope, report instead).
- The lookbook `--check` keeps failing after regeneration (indicates nondeterministic story content — do not "fix" by fuzzing the SVGs).
- `meta.json` schema is unclear (pages not listed there but ordering breaks in the docs build).

## Maintenance notes

- The catalog table now enumerates every shared visual component; any PR adding a module under `crates/jackin-tui/src/components/` must add a row + lookbook story + docs page (this is the existing documented coverage rule — now actually satisfiable because the baseline is true).
- Reviewer should scrutinize: catalog descriptions vs module doc comments (no invented capabilities), and that the regenerated SVGs only add files / change the two touched stories.
- Deferred: whether `agent_choice` should be *made* two-button or stay a list picker — tracked as its own follow-up decision (see plans/README.md "Findings considered"); this plan only makes the docs stop lying about it.
