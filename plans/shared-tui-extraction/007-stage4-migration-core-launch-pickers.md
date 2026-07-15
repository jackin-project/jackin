# Plan 007: Migrate `jackin❯` slices 1–3 — core inversion, launch TUI, pickers (Stage 4, part 1 of 2)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 006 is DONE and records the immutable Stage-3 TermRock revision. In `jackin❯`: `git branch --show-current` → `feature/shared-tui-extraction`; run the stage-boundary donor-drift check (Step 1). Compare the plan-001 "Current state" excerpts for `crates/jackin-tui/src/lib.rs:28-32`, `scroll.rs:20`, `ansi.rs:14,193` against live code; mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED-HIGH (first real consumer edits; 195-file blast radius begins)
- **Depends on**: plans/shared-tui-extraction/006-stage3-revision-consumable-quality.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

First half of **Stage 4** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 4: Migrate the first consumer"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx) and [ch. 08, "`jackin❯` migration slices"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx): pin TermRock, then migrate in dependency order — slice 1 (lower-level `jackin-core`/runtime definitions + dependency-inversion removal), slice 2 (lookbook and launch TUI), slice 3 (console-oppicker and picker facades). Migrating bottom-up prevents a high-level surface from depending on two owners of one primitive. Console and Capsule (the two biggest surfaces) follow in plan 008.

## Current state

Verified at freeze (plan 001 evidence; re-verify via drift check):

- Consumer inventory (195 files referencing `jackin_tui`): `jackin-console` 105, `jackin-capsule` 42, `jackin-launch-tui` 26, root `jackin` 11, `jackin-console-oppicker` 4, `jackin-runtime` 2, `jackin-diagnostics` 2, `jackin-core` 2, `jackin-host` 1.
- Dependency-inversion lines to remove: `crates/jackin-tui/src/lib.rs:28` (`shorten_home` re-export — helper **stays in `jackin-core`**, only the re-export dies; callers import `jackin_core::shorten_home` directly), `lib.rs:29-32` + `scroll.rs:20` (scroll/chrome family — callers move to `termrock::scroll`/`termrock::layout`), `ansi.rs:14` + `ansi.rs:193` (pointer/OSC52 — callers move to `termrock::osc`).
- Donor `runtime.rs` Tokio parts (receiver `Subscription` impls, spawn helpers, fallback runtime) re-home to `jackin-console`, connecting to `termrock::runtime::Subscription` via its closure adapter ([Decision 3](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).
- Migration rules ([ch. 08, "Cross-repository change graph"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)): **no `pub use termrock::*` compatibility facade in `jackin-tui`**, no type duplicated under both owners, donor coexists only for not-yet-migrated modules; every slice pins a full TermRock revision, commits `Cargo.lock`, and reruns the whole workspace.
- Verification harness: `cargo xtask ci --fast` (skips feature-powerset/Docker), full `cargo xtask ci` at slice boundaries; lookbook drift job in `ci-required` (plan 001); parity fixtures = donor `TestBackend` tests + 29 SVGs + `evidence/render-manifest.json`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Whole-workspace fast gate | `cargo xtask ci --fast` | exit 0 |
| Full gate (slice boundary) | `cargo xtask ci` | exit 0 |
| Remaining donor references | `rg -l 'jackin_tui' --glob '*.rs' \| grep -v '^crates/jackin-tui' \| wc -l` | strictly decreasing per slice |
| Inversion check | `rg -n 'jackin_core' crates/jackin-tui --glob '*.rs'` | empty after slice 1 |
| Donor SVG parity | `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` | exit 0 while donor lookbook exists |
| Docs gates | `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` | exit 0 |

## Scope

**In scope** (`jackin❯` workspace, branch `feature/shared-tui-extraction`): root `Cargo.toml`/`Cargo.lock` (termrock Git dependency), `crates/jackin-core` (remove now-duplicated neutral TUI helpers after callers migrate), `crates/jackin-tui` (delete migrated modules/re-exports), `crates/jackin-launch-tui`, `crates/jackin-console-oppicker`, `crates/jackin-runtime`, `crates/jackin-diagnostics`, `crates/jackin-host`, `jackin-console` (only the re-homed Tokio runtime helpers in this plan), root `jackin` package files touched by slice-1 items.

**Out of scope**: `jackin-console` UI surfaces and `jackin-capsule` (plan 008); donor crate deletion (plan 008); product redesign of any screen; quality fixes; TermRock changes except recorded-and-repinned fixes (see Git workflow).

## Git workflow

- All commits on `feature/shared-tui-extraction`, signed, Conventional Commits, pushed immediately. One commit (or small group) per slice; each slice ends with the whole workspace green — no slice merges independently ([Decision 18](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).
- TermRock dependency form is fixed: `termrock = { git = "https://github.com/tailrocks/termrock.git", rev = "<full-40-char-sha>" }` + committed `Cargo.lock`. Never `branch =`. Every repin gets recorded in `evidence/stage4-slices.md` and reruns the workspace.
- If migration exposes a TermRock defect: fix it in the extraction clone as a new signed commit, push TermRock `main` forward after its gates pass, repin `jackin❯` to the new SHA. Rendering-behavior changes still require the backlog protocol.

## Steps

### Step 1: Stage-boundary drift check and pin

1. `git fetch origin && git log --oneline <frozen-donor-rev>..origin/main -- crates/jackin-tui crates/jackin-tui-lookbook crates/jackin-core` — if upstream touched donor modules/consumers/fixtures, merge-sync (`git merge --no-ff origin/main -m "chore(merge): sync main into feature/shared-tui-extraction"`), regenerate affected plan-001 evidence, and port changes to their decided owner per [Decision 20](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx) before proceeding.
2. Add the TermRock dependency pinned to the plan-006 immutable revision in the workspace `Cargo.toml` (workspace-level dependency table, matching how other workspace deps are declared); `cargo update -p termrock` no-op check; commit `Cargo.lock`.

**Verify**: `cargo xtask ci --fast` → exit 0 with the new dependency present but unused; `rg -n 'termrock.*branch' Cargo.toml` → empty.

### Step 2: Slice 1a — remove the dependency inversion

1. Migrate the two `jackin-core` files referencing `jackin_tui` plus every caller of the five re-export lines: callers of `jackin_tui::{TailScroll,is_scrollable,max_line_width,max_offset,DialogBodyScroll,StatusFooterHover,BOTTOM_CHROME_ROWS,BottomChromeAreas,bottom_chrome_areas}` → `termrock::{scroll,layout}`; callers of `jackin_tui::shorten_home` → `jackin_core::shorten_home`; callers of pointer/OSC52 items → `termrock::osc` (emission sites keep their current policy — typed request + consumer-side encoder call must produce the same bytes the raw helper produced).
2. Delete the five re-export lines from `crates/jackin-tui/src/{lib.rs,scroll.rs,ansi.rs}`; delete the now-orphaned duplicated neutral definitions from `jackin-core` once `rg -n 'TailScroll|bottom_chrome_areas|DialogBodyScroll|StatusFooterHover' --glob '*.rs'` shows no remaining callers of the `jackin-core` copies (except `jackin-core`-internal use that legitimately remains — investigate each hit; `shorten_home` and path-display policy stay).

**Verify**: `rg -n 'jackin_core' crates/jackin-tui --glob '*.rs'` → empty; `cargo xtask ci --fast` → exit 0; `rg -n 'pub use termrock' crates/jackin-tui/src` → empty (no facade).

### Step 3: Slice 1b — re-home Tokio runtime helpers, migrate `runtime`/small crates

1. Move donor `runtime.rs` Tokio receiver impls/spawn helpers/fallback runtime into `jackin-console` (target home per [ch. 02](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx)); wrap receivers via `termrock::runtime`'s closure adapter (or local newtypes); executor-neutral contract callers move to `termrock::runtime` (`Dirty`, `UpdateResult`, `Component`, `View`, `drive_frame`, `drive_render`).
2. Migrate the small consumers: `jackin-runtime` (2 files), `jackin-diagnostics` (2), `jackin-host` (1), and the root `jackin` package's 11 files where they use slice-1 items (defer any file that mainly composes console/Capsule widgets to plan 008 — record the deferral in `evidence/stage4-slices.md`).
3. Delete donor `runtime.rs` once its inverse-dependency query is empty: `rg -ln 'jackin_tui::runtime|use jackin_tui::.*drive_frame' --glob '*.rs'` → empty.

**Verify**: `cargo xtask ci --fast` → exit 0; console/Capsule/launch still build against remaining donor modules; commit + push; append slice record (files migrated, TermRock rev, gate results) to `evidence/stage4-slices.md`.

### Step 4: Slice 2 — launch TUI and lookbook consumers

1. Migrate `jackin-launch-tui` (26 files): imports move to `termrock::{widgets,style,text,input,interaction,layout,scroll}` per the plan-006 migration guide's path table. Launch-specific animation (`animation.rs` family) stays donor/product-local — launch keeps using it from `jackin-tui` until plan 008 relocates remain-local modules.
2. Behavioral parity for launch: run existing launch-tui tests; render its lookbook/SVG stories if any exist donor-side; manual smoke via `cargo run -- …` launch path if TESTING.md defines one (`jackin❯` runs are containerized; use the documented test-runner path only).
3. Donor lookbook: `jackin❯`'s product stories keep working against the donor crate for now. Do **not** delete `jackin-tui-lookbook` yet — [ch. 04 Stage 4](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx) allows removal only after equivalent external terminal/render/check workflows pass; that deletion happens in plan 008 with the docs transfer.

**Verify**: `cargo xtask ci --fast` exit 0; `rg -l 'jackin_tui' crates/jackin-launch-tui --glob '*.rs'` → empty; donor SVG check still green; slice record appended; commit + push.

### Step 5: Slice 3 — oppicker and picker facades

Migrate `jackin-console-oppicker` (4 files) and product-local picker facades that only consume already-extracted widgets (select-list/list, text-input/filter preset, dialogs): rows become borrowed `ListRow`-shaped data with stable IDs; product filtering supplies the consumer-side strategy; OnePassword/role/mount/scope wording stays local ([ch. 02, "Keep in jackin❯"](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx)).

**Verify**: `cargo xtask ci` (full) → exit 0; `rg -l 'jackin_tui' crates/jackin-console-oppicker --glob '*.rs'` → empty; consumer-file count strictly below the slice-2 count; slice record appended; commit + push.

### Step 6: Interim parity checkpoint

1. Re-run donor `TestBackend` suites for migrated surfaces + `termrock` parity buffer tests; diff donor SVG fixtures (`--check` job) — all byte-identical.
2. Any parity diff must trace to a `quality-backlog.md` item; an untraceable diff fails Stage 4 ([ch. 08, "Parity matrix"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx)) — revert the offending slice commit rather than adjusting fixtures.
3. Update `evidence/stage4-slices.md` with the checkpoint summary; commit + push; note progress on PR #794.

**Verify**: gates green; evidence committed.

## Test plan

- No donor test may be deleted in this plan except tests of code that physically moved (their moved equivalents must pass in the new home — e.g. runtime driver tests now live in TermRock, Tokio-helper tests move to `jackin-console`).
- Slice-level regression surface: full `cargo xtask ci` at each slice boundary; targeted `cargo nextest run -p <migrated-crate>` during work.
- OSC emission parity: unit-test that consumer emission sites produce byte-identical sequences via `termrock::osc` encoders vs. the old `jackin-core` helpers (fixtures from plan 005's encoder tests).

## Done criteria

- [ ] Workspace pins one full TermRock `rev`; `Cargo.lock` committed; no `branch =` dependency anywhere
- [ ] `rg -n 'jackin_core' crates/jackin-tui --glob '*.rs'` → empty; no `pub use termrock` facade in `jackin-tui`
- [ ] `jackin-launch-tui`, `jackin-console-oppicker`, `jackin-runtime`, `jackin-diagnostics`, `jackin-host`, `jackin-core` contain zero `jackin_tui` references
- [ ] No neutral helper duplicated across `jackin-core`/`termrock` for any migrated item
- [ ] Donor SVG drift check and all migrated-surface fixtures byte-identical; every diff (expected: none) traced to the quality backlog
- [ ] `cargo xtask ci` (full) green at final slice; every commit signed + pushed
- [ ] `evidence/stage4-slices.md` records every slice: files, TermRock rev, gate results
- [ ] Index row → DONE (roadmap Stage 4 checkbox stays unticked until plan 008 completes the stage)

## STOP conditions

- A parity diff does not trace to a recorded quality-backlog item.
- A migration forces changing a TermRock public API in a way ch. 06/09 prohibit (product noun, index identity, escape bytes) — the extraction missed a contract; operator review.
- You need a compatibility re-export facade to make progress — prohibited; migrate callers instead.
- Upstream `origin/main` merge-sync conflicts touch donor modules in ways that invalidate frozen evidence and the re-freeze protocol would restart Stage 0 — report before regenerating anything.
- `cargo xtask ci` failure that persists after two focused fix attempts in the migrated slice.

## Maintenance notes

- Keep the consumer-file count trend in the slice records — plan 008's donor deletion requires it to reach the console/Capsule-only remainder.
- Reviewers: the OSC emission-parity tests and the `jackin-core` duplicate-deletion query results are the two places silent divergence hides.
- Deferred: console/Capsule migration, docs transfer, donor deletion (plan 008); Capsule `Tabs`/`HintBar` convergence (post-roadmap).
