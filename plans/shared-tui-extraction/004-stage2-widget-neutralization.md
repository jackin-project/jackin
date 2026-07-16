# Plan 004: Neutralize and redesign donor widgets (Stage 2, part 2 of 3)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 003 is DONE; in the extraction clone `cargo nextest run --workspace` passes and modules `text/input/interaction/layout/scroll/style` exist. Nothing pushed to `tailrocks/termrock` yet.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED-HIGH (largest behavioral surface; parity depends on bug-compatibility discipline)
- **Depends on**: plans/shared-tui-extraction/003-stage2-workspace-foundations.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

Second third of **Stage 2** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx). It lands **Batch B (leaf widgets)** and **Batch C (stateful widgets)** from [ch. 08, "Refactoring order"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx), applying the per-component contracts in [ch. 09 — Component Redesign Catalog](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) exactly. This is where the donor's product-shaped signatures become the reusable API: stable IDs instead of indices, borrowed render data, consumer-owned wording/validation, `Widget for &T`/`StatefulWidget for &T`, and semantic theme roles — all while keeping rendered bytes **bug-compatible** so `jackin❯` parity (plan 008) can be byte-identical.

## Current state

In the extraction clone after plan 003, `crates/termrock/src/components/` still holds the donor component files (names verified at freeze): `bottom_chrome.rs` (folded into `layout::slots` by plan 003), `brand_header.rs`, `button_strip.rs`, `confirm_dialog.rs`, `container_info.rs`, `dialog_layout.rs`, `diff_view.rs`, `error_dialog.rs`, `filter_input.rs`, `focus_owner.rs` (split by plan 003), `hint_bar.rs`, `hover_tracker.rs` (moved by plan 003), `modal_backdrop.rs`, `modal_lifecycle.rs`, `modal_rects.rs` (replaced by plan 003 `layout::dialog`), `panel.rs`, `save_discard_dialog.rs`, `scrollable_panel.rs`, `select_list.rs`, `status_footer.rs`, `status_popup.rs`, `tab_strip.rs`, `text_input.rs`, `toast.rs`, plus the `components.rs` facade.

Key donor signature problems recorded in [ch. 02, "Signatures that must change"](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx) and [ch. 06, "Donor refactoring ledger"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx):

- `select_list.rs` — `SelectListState` owns `Vec<String>`, reports indices, hardcodes ASCII substring filtering and action labels, measures width with `chars().count()`.
- `text_input.rs` — `TextField` mixes cursor/edit state, forbidden-value validation, save/cancel wording, Crossterm events.
- `status_footer.rs` — names container/run/usage concepts with fixed right-side order.
- `container_info.rs` — `ContainerInfoState`/`DebugInfo` mix reusable detail rows with agent/role/container/version fields.
- `dialog_layout.rs` — combines shell rendering, scrolling, key hints, axes, Capsule byte-wheel behavior.
- Rendering today is a mixture of free `render_*` functions, `Frame` wrappers, and consuming widgets — must become one convention.

Binding decisions ([Decision 19](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx), [ch. 09](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx)): no public `FilterField` (filter = documented `TextInput` composition); no modal stack/parent-chain/router (only `Backdrop`, dialog geometry, pure inside/outside hit classification); no public `status_popup` type (folds into message/toast surface); `save_discard_dialog` = consumer preset over `ChoiceDialog<Id>`; text editing on extended grapheme clusters + `unicode-width` columns ([Decision 16](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)) — but the *recorded width defects stay bug-compatible* until plan 009 (grapheme-safe cursor mechanics are structural and must exist now; the four char-count **measurement** sites listed in the quality backlog keep their current math until post-parity).

## Commands you will need

Same as plan 003 (extraction clone): `cargo check`/`nextest`/`clippy`/`fmt`/`deny`/`reuse`, plus `cargo check -p termrock --no-default-features` and the `cargo tree` forbidden-dep grep after every batch.

## Scope

**In scope** (extraction clone): `crates/termrock/src/widgets/` (create per [ch. 09 target tree](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx): `action_bar.rs`, `detail_table.rs`, `dialog.rs`, `diff.rs`, `hint_bar.rs`, `list.rs`, `panel.rs`, `status_bar.rs`, `tabs.rs`, `text_input.rs`, `toast.rs`); deletion of `components.rs` mega-facade and product-only components; unit/buffer tests.

**Out of scope**: `runtime`/`osc`/`crossterm` modules and lookbook (plan 005); any public push; any `jackin❯` workspace change; quality-backlog fixes (the four width-measurement sites and color-only panel focus stay **unchanged**); new components beyond the donor set; `serde`/`tokio` features.

## Git workflow

Extraction clone `main`: one DCO-signed, buildable, Conventional-Commits commit per component (or tight component family). `feat(widgets): …` / `refactor(widgets): …`.

## Steps

### Step 1: Batch B — leaf widgets (extract after bounded parameterization)

Apply the [ch. 09 "Extract after bounded parameterization" table](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) row by row; each component gets the standard anatomy (render data / interaction state / action / outcome / layout / widget) and the eleven contract rules from "Standard component anatomy":

1. `panel.rs` → `widgets::Panel`: semantic focus/emphasis roles instead of phosphor constants; accept `Block`/builder-lite overrides; delete `modal_block`/`unfocused_block` policy helpers. Keep the color-only focus border **as-is** (backlog item).
2. `modal_backdrop.rs` → `widgets::Backdrop`: semantic style/char policy; `Widget for &Backdrop`.
3. `hint_bar.rs` → `widgets::HintBar`: borrowed typed hint descriptors (chord display + caller label), wrapping + priority/visibility, display-width measurement kept bug-compatible; wrapping must cover Capsule's wrapped-hint behavior (per [ch. 02, "Parameterize before publishing"](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx)) so Capsule can converge post-parity.
4. `button_strip.rs` → `widgets::ActionBar<Id>`: borrowed items (stable ID, label, enabled, optional style override); focused ID in state; activated-ID outcome; hit regions from the same layout fn; remove duplicated line/style/rect helpers.
5. `toast.rs` → `widgets::Toast`: borrowed message/severity/anchor/style; pure geometry; timers/queues stay consumer-side.
6. `tab_strip.rs` → `widgets::Tabs<Id>`: borrowed stable-ID items with label/active/enabled; selected/hovered/focused in separate state; hit regions keyed by ID; **per-tab glyph/state slots** (the Capsule status bar re-implements tab painting today precisely because these are missing).
7. Delete `brand_header.rs` from the TermRock tree (stays `jackin❯`-local; ch. 09 "Keep consumer-local").

Convert every moved widget to `Widget for &T` / `StatefulWidget for &T`; delete the public duplicate `render_*` wrappers as each converts.

**Verify after each component**: `cargo nextest run -p termrock` all pass; component buffer tests (see Test plan) assert unchanged rendered cells vs. donor for equivalent inputs.

### Step 2: Batch C — stateful widgets (reimplement behind the same contract)

Apply the [ch. 09 "Reimplement before extraction" table](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) row by row:

1. `select_list.rs` → `widgets::List<Id>`/`SelectList<Id>`: borrowed rows (`ListRow<'a, Id>`-shaped: id, label `Line`, role, enabled); query/selection/scroll in `ListState<Id>`; consumer-supplied visible projection or filtering strategy (no built-in ASCII substring filter); display-width windows (**keeping the recorded char-count label-measurement defect** until plan 009); `ListOutcome<Id>::{Ignored,Changed,Activated(Id)}`; visible-row virtualization (no full-collection clone per frame — [ch. 08 performance gate](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)).
2. `text_input.rs` → `widgets::TextInput` + `TextInputState`: grapheme-safe editing (cursor as byte offsets validated against `unicode-segmentation` boundaries; movement/backspace/delete/selection/viewport on grapheme clusters per [Decision 16](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)); logical `EditAction`s; external validation result/message as inputs; submit/cancel mapping consumer-owned; cursor placement exposed as render metadata. Also add the documented **filter composition/preset** (lookbook story + docs in plans 005/006) replacing `filter_input.rs`; delete `filter_input.rs`.
3. `scrollable_panel.rs` → thin widgets over `scroll::ViewportState` (plan 003): borrowed visible content or row source; `StatefulWidget`; thumb/hit geometry exposed; delete the duplicated scroll math and public low-level render functions.
4. `dialog_layout.rs` → `widgets::Dialog` + `layout::DialogSpec`: separate responsive geometry, scroll state, shell widget, logical actions; caller supplies title/body/footer/action bar; Capsule byte-wheel handling stays consumer-side (backend adapters translate wheel events).
5. `confirm_dialog.rs` → `widgets::ChoiceDialog<Id>`: borrowed title/body/warning/action descriptors; stable action IDs; choice/cancel outcome; exit wording and data-loss constructors stay `jackin❯`-local.
6. `save_discard_dialog.rs` → delete as a public type; document Save/Discard/Cancel as caller action data over `ChoiceDialog<Id>`.
7. `error_dialog.rs` → `widgets::MessageDialog` composed with `DetailTable`: borrowed message + neutral detail rows; typed `Copy(Id)`/`ActivateLink(Id)` outcomes and hit regions. Raw OSC overlay bytes remain temporarily as a private seam replaced in plan 005's `osc` module — mark with `// TODO(osc)`.
8. `container_info.rs` → `widgets::DetailTable<Id>`: label/value rows, selection, scrolling, wrapping, copy/link capabilities, typed outcomes. `DebugInfo` and all product row construction **deleted from TermRock** (stays donor-side for Stage 4).
9. `status_footer.rs` → `widgets::StatusBar<Id>`: borrowed left/right slots (stable ID, priority, min width, truncation rule, emphasis, enabled); hit regions/activated ID; usage/container/run policy and ordering stay `jackin❯`-local. Keep the right-group char-count defect **as-is**.
10. `status_popup.rs` → delete; behavior folds into `MessageDialog`/`Toast` composition.
11. `diff_view.rs` → `widgets::DiffView` + `DiffState`: parse/project outside rendering; borrowed immutable hunks/lines; only selection+scroll in state; semantic added/removed roles; render returns nothing and mutates nothing beyond Ratatui state.
12. `modal_lifecycle.rs` → keep only pure backdrop + inside/outside hit classification (already in `interaction`/`layout` from plan 003); delete stack/parent-chain lifecycle.

**Verify after each component**: workspace green; buffer tests assert donor-equivalent cells; `rg -n 'chars\(\)\.count\(\)' crates/termrock/src/widgets` still shows the backlog sites (bug-compatibility proof — count must match the quality-backlog list, no more, no fewer).

### Step 3: Delete the `components.rs` facade and finalize the module tree

Delete `crates/termrock/src/components.rs` and any remaining `components/` files; `lib.rs` exposes the [ch. 06 module set](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx): `text`, `input`, `interaction`, `layout`, `scroll`, `style`, `widgets` (plus `osc`/`runtime`/`crossterm` arriving in plan 005). No `prelude`, no glob re-exports; root re-export only for `Theme`.

**Verify**: `rg -n 'pub use .*\*' crates/termrock/src/lib.rs` → no matches; `rg -n '^pub mod' crates/termrock/src/lib.rs` → exactly the module list above.

### Step 4: API-neutrality sweep

Run the [ch. 09 "API rejection checks"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) denylist as an audit aid over public items: `rg -in 'agent|capsule|container|workspace|role|run_id|database|trace|job|credential|connection' crates/termrock/src --glob '*.rs'` — every hit must be in a private impl detail, test fixture being neutralized, or doc example; no public signature/doc may carry a product noun. Also: no public collection indices as durable identity, no raw escape bytes as widget outcomes (except the marked `TODO(osc)` seam), no hardcoded action labels, no `&mut` render for hidden projection.

**Verify**: documented sweep results appended to the extraction ledger's notes column for each component; zero public-surface hits.

### Step 5: Gate sweep + checkpoint evidence

Run the full plan-003 command table. Append the checkpoint (clone tip SHA, per-component status, gate results) to `plans/shared-tui-extraction/evidence/stage2-checkpoints.md` in `jackin❯`; commit + push (`chore(tui): record termrock stage 2 widgets checkpoint`).

**Verify**: all gates green; evidence committed.

## Test plan

Per extracted/reimplemented component, before plan 005 (extraction-tier conformance from [ch. 09, "Per-component conformance"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx)):

- direct `Buffer` tests: default, focused, disabled, empty, overflow, tiny (1×1), and off-origin rectangles — zero/tiny areas must not panic;
- update tests: ignored actions don't mutate state; handled actions return the documented outcome;
- stable-ID tests across insertion/removal/sorting/filtering (List, Tabs, ActionBar, StatusBar, DetailTable, ChoiceDialog);
- layout/hit-test agreement at minimum, normal, wide sizes (the layout fn that paints is the one that yields regions);
- grapheme corpus for `TextInputState` mechanics: combining marks, emoji ZWJ, emoji modifiers, regional indicators, CJK wide, zero-width, clipping at both viewport ends ([Decision 16](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx));
- donor-parity buffer tests: for inputs expressible in both APIs, the new widget's rendered cells equal the donor's (seed from the donor's existing `TestBackend` tests — 18 donor files use `TestBackend`; keep them as the pattern).

Quality-tier tests (non-color cues, corrected width math) are **deliberately absent** until plan 009.

Verification: `cargo nextest run --workspace --all-features --locked` → all pass.

## Done criteria

- [ ] Every donor component file has its ch. 09 disposition executed: extracted, reimplemented, folded, or deleted-as-local — cross-checked row-by-row against the extraction ledger, ledger updated with final TermRock paths
- [ ] `components.rs` facade gone; module tree matches ch. 06/09 (minus `osc`/`runtime`/`crossterm`)
- [ ] All rendering is `Widget for &T` / `StatefulWidget for &T`; no public free `render_*` duplicates (`rg -n 'pub fn render_' crates/termrock/src` → empty)
- [ ] No public product nouns, no public index-based identity, no hardcoded action wording
- [ ] Quality-backlog defect sites present and unchanged (char-count math count matches backlog; panel focus still color-only)
- [ ] Full gate sweep green incl. `no-default-features` isolation and forbidden-dep grep
- [ ] Every commit DCO-signed, buildable; nothing pushed to `tailrocks/termrock`
- [ ] Checkpoint evidence committed in `jackin❯`; index row → DONE

## STOP conditions

- A redesign cannot preserve donor-rendered bytes for equivalent inputs (outside recorded backlog items) — that is a hidden behavior change; record and stop.
- You need a new public type not in ch. 09's dispositions (e.g. a modal stack "just for now") — that reopens a closed decision.
- A component turns out to need Tokio, Crossterm types, or I/O in its base contract.
- The grapheme-safe cursor mechanics force a visible rendering change beyond the recorded backlog (would break parity — report; likely needs an operator-reviewed backlog addition per [Decision 13](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).

## Maintenance notes

- Exact public names may still improve in plan 006's bounded API review — avoid premature doc-churn on names flagged "illustrative" in ch. 06/09.
- Reviewers: the highest-risk rows are `select_list` → `List<Id>` (index→ID identity swap) and `text_input` (grapheme mechanics vs. bug-compatible measurement) — check the parity buffer tests actually pin donor bytes.
- Deferred: `error_dialog`'s raw-overlay seam (plan 005 `osc`), all quality fixes (plan 009), Capsule convergence on `Tabs`/`HintBar` (post-roadmap follow-up).
