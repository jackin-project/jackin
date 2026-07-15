# Plan 008: Migrate console + Capsule, prove parity, retire the donor (Stage 4, part 2 of 2)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 007 is DONE; `evidence/stage4-slices.md` shows slices 1–3 complete with the consumer inventory reduced to `jackin-console`, `jackin-capsule`, and remaining root-`jackin` files; donor SVG check green.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH (donor deletion is the point of no return for in-tree rollback)
- **Depends on**: plans/shared-tui-extraction/007-stage4-migration-core-launch-pickers.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

Completes **Stage 4** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 4"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx) and [ch. 08's slices 4–6](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx): migrate the host console in vertical slices, migrate Capsule with explicit raw-byte and bottom-chrome parity, transfer generic docs to the external catalog, and delete the in-tree donor **only after** the full parity matrix passes and the inverse dependency query is empty. Roadmap acceptance is explicit: "`jackin❯` render and behavior parity passes before any in-tree donor code is removed."

## Current state

After plan 007: remaining `jackin_tui` consumers are `jackin-console` (105 files at freeze), `jackin-capsule` (42), residual root-`jackin` files (≤11), plus the donor crates themselves. Donor still contains: remain-local modules (`ownership.rs`, `host_colors.rs`, `url_text.rs`, `prune_output.rs`, `output.rs`, `animation.rs`, `brand_header`, product dialogs' wording constructors, `DebugInfo` builder, Capsule raw-byte decoder, product palette constants, product stories) and any component still consumed by console/Capsule.

Key parity constraints:

- **Capsule keeps its locally painted status bar and hand-painted hint rows** through migration ([ch. 08, migration slices](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)); convergence on shared `Tabs`/`HintBar` is a post-parity follow-up, never a migration requirement.
- Capsule surfaces receive raw bytes, not Crossterm events — the byte decoder feeds `termrock::input::KeyChord` so byte-decoded and Crossterm inputs share one dispatch path ([ch. 06, "State, input, and effects"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx)); the decoder itself stays `jackin❯`-local.
- Terminal-ownership process globals (rich-surface/host-screen atomics in donor `ownership.rs`) and title/alternate-screen policy stay product-local — they relocate into a `jackin❯` crate (console or a small local module), never into TermRock ([ch. 02](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx)).
- Product palette constants (`CAPSULE_MENU_*`, `BRAND_BLOCK`, `DEBUG_AMBER`, `TAB_BG_*`, rain colors) become local constants layered above `termrock::style::Theme` ([ch. 06, "Theme API"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx)).
- Docs state at freeze: generic lookbook pages live at `docs/content/docs/reference/tui/lookbook/*.mdx` with 29 SVGs under `docs/public/tui-lookbook/`; canonical TUI design pages under `docs/content/docs/reference/tui/`. The TermRock catalog (plan 006) now owns the generic content.

## Commands you will need

Plan 007's table, plus:

| Purpose | Command | Expected on success |
|---|---|---|
| Capsule smoke lane | `cargo xtask ci --e2e` (Docker required) | exit 0 |
| Inverse dependency query | `rg -l 'jackin_tui' --glob '*.rs'` | empty before donor deletion |
| Workspace member check | `cargo metadata --format-version 1 \| python3 -c "…names…"` | no `jackin-tui`, `jackin-tui-lookbook` |
| Docs build + links | `cd docs && bun run build && bun run check:links:fresh` | exit 0 |
| Roadmap/research sidebars | `cargo xtask roadmap audit && cargo xtask research check` | exit 0 |

## Scope

**In scope**: `crates/jackin-console` (105 files), `crates/jackin-capsule` (42), residual root `jackin` files, relocation targets for remain-local donor modules, `docs/content/docs/reference/tui/**` + `docs/public/tui-lookbook/**` (transfer/deletion), `docs/content/docs/reference/getting-oriented/codebase-map.mdx` and other canonical docs naming donor crates, deletion of `crates/jackin-tui` + `crates/jackin-tui-lookbook`, `.github/workflows/ci.yml` (retire the donor lookbook drift job in the same commit as donor deletion), workspace `Cargo.toml`.

**Out of scope**: any visible redesign (parity is byte-identical); Capsule status-bar/hint convergence; quality fixes (plan 009); TermRock tag/protection (plan 009).

## Git workflow

Same as plan 007: signed slice commits on `feature/shared-tui-extraction`, pushed immediately, whole workspace green per slice, TermRock repins recorded. Donor deletion is its own commit (`refactor(tui)!: retire in-tree jackin-tui donor after termrock parity`) so rollback-by-revert stays clean until merge.

## Steps

### Step 1: Slice 4 — host console in vertical component-family slices

Migrate `jackin-console` (and its remaining root-`jackin` compositions) in vertical slices by shared component family, per [ch. 08 slice 4](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx). Suggested family order (leaf → stateful, mirroring extraction batches): panels/backdrops/toasts → tabs/action bars/hint bars → status bar slots (console composes `StatusBar<Id>` slot data locally; usage/container/run policy local) → lists/select-lists/filter preset → text inputs/dialogs (exit wording, data-loss constructors local via `ChoiceDialog<Id>` presets) → detail tables (`DebugInfo` builder stays local, renders through `DetailTable<Id>`) → diff view → scrollable panels/viewports. For each family: swap imports to `termrock::…`, convert index-based call sites to stable IDs, supply consumer-side wording/validation/filtering, keep every screen's rendered bytes identical; run that family's `TestBackend` tests before moving on.

Relocate remain-local donor modules when their last console consumer migrates: `ownership.rs` (process globals + title policy), `host_colors.rs`, `url_text.rs`, `prune_output.rs`/`output.rs`, `animation.rs`, `brand_header`, product palette constants — into `jackin-console`/root-`jackin` modules (follow the codebase map's existing layout conventions; `src/console/tui/` is the host-console TUI surface per the root CLAUDE.md table).

**Verify per family**: `cargo xtask ci --fast` → exit 0; family fixtures byte-identical. After the last family: `rg -l 'jackin_tui' crates/jackin-console src --glob '*.rs'` → empty; full `cargo xtask ci` → exit 0; slice records appended; commit + push per family.

### Step 2: Slice 5 — Capsule

1. Make raw-byte→logical-key parity explicit first ([ch. 08 slice 5](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)): the Capsule byte decoder (donor-side `keymap.rs` remainder) moves into `jackin-capsule`, emitting `termrock::input::KeyChord`; add decoder tests asserting identical chord output for the existing byte-sequence fixtures.
2. Make bottom-chrome parity explicit: Capsule's status bar and wrapped hint rows stay **hand-painted locally** — port their painting code into `jackin-capsule` unchanged where it currently rides on donor helpers; where it used donor primitives (display-width, scroll, panel), rebase onto `termrock` foundations without changing painted bytes.
3. Migrate the remaining 42 files by the same family method as Step 1.
4. Run the Capsule smoke mandate: `cargo xtask ci --e2e` (Docker-backed lane per TESTING.md/CI rules).

**Verify**: `rg -l 'jackin_tui' crates/jackin-capsule --glob '*.rs'` → empty; `cargo xtask ci --e2e` → exit 0; slice record; commit + push.

### Step 3: Full parity matrix

Execute every row of the [ch. 08 parity matrix](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx) and record results in `evidence/stage4-parity.md`: TestBackend cells/styles; SVG previews (donor fixtures vs. product compositions rendered through termrock — byte-identical); key dispatch (keymap tests + adapter contract tests); focus/hover; Unicode display-column tests; resize min/normal/wide + zero/tiny no-panic; terminal cleanup (partial-init/restore/drop/PTY — TermRock side already green, `jackin❯` ownership policy tests local); process policy (rich-surface/diagnostics tests still entirely local); OSC 8/22/52 typed-request emission parity; dependency graph (no product/Tokio/Crossterm in base termrock graph — re-run from the pinned rev); feature additivity (component inventory + buffer hashes identical with/without `crossterm`); docs (external catalog paths live).

Every diff must trace to a quality-backlog item; expected count: zero.

**Verify**: `evidence/stage4-parity.md` complete, all rows PASS; commit + push.

### Step 4: Docs transfer

1. Generic component/lookbook content: delete `docs/content/docs/reference/tui/lookbook/*.mdx` and `docs/public/tui-lookbook/*.svg`, replacing inbound references with links to the external TermRock catalog (external links are fine for external repos per docs rules). Update `docs/content/docs/reference/tui/index.mdx` and related pages to state shared-vs-`jackin❯` ownership for every component and policy (roadmap acceptance: "one owner for every component and policy").
2. `jackin❯` keeps: product interaction/composition decisions, product stories rationale, Capsule-specific behavior docs — linking out to TermRock convention pages instead of duplicating ([ch. 03, "Fumadocs component catalog"](../../docs/content/docs/reference/research/shared-tui-extraction/03-target-repository.mdx)).
3. Update `codebase-map` and any page naming `jackin-tui`/`jackin-tui-lookbook`/`tui-lookbook` CLI: `rg -rn 'jackin-tui|tui-lookbook' docs/content AGENTS.md CLAUDE.md TESTING.md PROJECT_STRUCTURE.md` and fix each (root CLAUDE.md TUI table row "Shared components | `crates/jackin-tui/src/`" → termrock reference; lookbook row → external).
4. Docs gates: `cd docs && bun run build && bun run check:links:fresh`; `cargo xtask docs repo-links`; `cargo xtask roadmap audit`; `cargo xtask research check`.

**Verify**: all four gates exit 0; no stale donor references (`rg -n 'jackin-tui' docs/content` → only historical research/roadmap dossier mentions, which stay as the record).

### Step 5: Pre-deletion merge-sync and donor deletion (slice 6)

1. Merge-sync per [Decision 20](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): `git fetch origin && git merge --no-ff origin/main -m "chore(merge): sync main into feature/shared-tui-extraction"`; if upstream touched any donor module/consumer/fixture/dependency/TUI doc, regenerate affected inventory + parity evidence and port changes to the decided owner before deleting anything.
2. Inverse dependency query must be empty: `rg -l 'jackin_tui' --glob '*.rs'` → no output (donor crates excluded once deleted).
3. Delete `crates/jackin-tui` and `crates/jackin-tui-lookbook`; remove from workspace `Cargo.toml` members and all `[workspace.dependencies]` entries; retire the donor lookbook drift job from `.github/workflows/ci.yml` (its external equivalents passed in plan 006) in the same commit; `cargo xtask ci` full.
4. Regenerate the consumer inventory into `evidence/stage4-parity.md`: `rg -l 'jackin_tui' --glob '*.rs' | wc -l` → 0 ([ch. 08, "`jackin❯` donor retired"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)).

**Verify**: `cargo metadata` shows no donor packages; full `cargo xtask ci` green; commit + push; PR #794 updated; tick roadmap Stage 4 checkbox (`docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx`) in this commit.

## Test plan

- Capsule byte-decoder chord-parity tests (Step 2.1) — new, with existing byte fixtures as input corpus.
- Family-by-family `TestBackend` byte-parity assertions — donor tests keep passing against migrated call sites until their subject moves; moved subjects' tests move with them.
- `cargo xtask ci --e2e` Capsule smoke (mandated by CI rules).
- The parity matrix (Step 3) is the acceptance test suite for this plan; its evidence file is the deliverable.

## Done criteria

"`jackin❯` donor retired" checklist from [ch. 08, "Completion gates"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx), verbatim:

- [ ] 195-file consumer inventory regenerated; zero `jackin_tui` imports remain
- [ ] No neutral helper duplicated in `jackin-core` or any product crate
- [ ] Console, Capsule, launch, picker, modal, mouse, Unicode, SVG, terminal-cleanup parity passes (`evidence/stage4-parity.md`)
- [ ] Product widgets/policies have explicit local owners (ownership stated in `jackin❯` TUI docs)
- [ ] Generic docs/previews point to the external catalog; product decisions remain in `jackin❯` docs; docs gates green
- [ ] `crates/jackin-tui` and `crates/jackin-tui-lookbook` deleted only after all above
- [ ] Full `cargo xtask ci` green; every commit signed + pushed; roadmap Stage 4 checkbox ticked
- [ ] Index row → DONE

## STOP conditions

- Any parity-matrix row fails or a diff lacks a quality-backlog trace — do not delete the donor; rollback path is repinning the last-good TermRock rev or retaining the not-yet-migrated donor module ([ch. 04, "Rollback"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx)).
- Capsule byte-decoder parity cannot be proven for some sequence — report with the failing fixture; do not fuzz-fix silently.
- The merge-sync brings donor-module changes that require re-freezing evidence — run the Decision 20 protocol and report before deletion.
- You are tempted to converge Capsule onto shared `Tabs`/`HintBar` "while you're there" — explicitly out of scope.
- Donor deletion would remove something still consumed (inverse query non-empty) — never force it.

## Maintenance notes

- After donor deletion, in-tree rollback is gone; fixes flow through TermRock repins. The unmerged PR can still revert the deletion commit until merge.
- Reviewers: the deletion commit should be almost pure deletion + workspace-manifest edits; any hunk editing surviving code deserves scrutiny.
- Follow-ups deliberately left open: Capsule `Tabs`/`HintBar` convergence (tracked post-parity item), bounded-lines projection graduation under the two-consumer rule.
