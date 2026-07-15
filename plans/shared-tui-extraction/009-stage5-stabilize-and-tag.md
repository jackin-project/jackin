# Plan 009: Stabilize TermRock — quality fixes, first tag, governance, roadmap retirement (Stage 5)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 008 is DONE: donor deleted, `evidence/stage4-parity.md` all-PASS, roadmap Stage 4 checkbox ticked, PR #794 still open (it must not merge before this plan's final step).

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (deliberate visual changes for the first time; release mechanics)
- **Depends on**: plans/shared-tui-extraction/008-stage4-console-capsule-donor-retirement.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

**Stage 5** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 5: Stabilize TermRock"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx). Parity won first; quality wins now ([Decision 13](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)): every recorded quality-backlog defect lands as its own reviewed TermRock change with regenerated fixtures and migration notes, `jackin❯` adopts each through deliberate repins that review the visual diff, the first annotated tag establishes the semver baseline ([Decision 21](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)), TermRock switches to protected-`main` governance, and the roadmap item is retired in the final commits of PR #794. Partial completion does not merge.

## Current state

- `jackin❯` (branch `feature/shared-tui-extraction`): donor deleted, fully on a pinned TermRock rev, all parity evidence green.
- TermRock `main`: green, unprotected (bootstrap direct-push mode), no tags.
- Quality backlog (`plans/shared-tui-extraction/evidence/quality-backlog.md`, frozen in plan 001): (a) character-count width math in `HintSpan::display_cols`-descended hint measurement, select-list label measurement, status-footer right group, error-dialog wrap math — fix = measure display columns via `unicode-width` over grapheme/string slices ([Decision 16](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)); (b) color-only panel focus border — fix = add a non-color cue (glyph/border-char/modifier) per the [ch. 06 non-color rule](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx); (c) any items appended during plans 002–008 via the re-freeze protocol.
- Quality-tier conformance still owed per component ([ch. 09, "Per-component conformance"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx)): Unicode/display-width cases with defects fixed + a non-color focus/selection/error cue, before the first tag.
- Lookbook donor-flag aliases (`--terminal`/`[out-dir]`/`--check <dir>` forms) still present from plan 005; scheduled for removal here ([Decision 2](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): no indefinite shims).

## Commands you will need

TermRock (extraction clone): plan 003/005/006 gate tables, plus `cargo semver-checks` (activates against the tag after it exists), `git tag -a v0.<y>.<z> -m …` + `git push termrock v0.<y>.<z>`.
`jackin❯`: `cargo xtask ci` (full), docs gates (`bun run build`, `check:links:fresh`, `cargo xtask docs repo-links`, `roadmap audit`, `research check`).

## Scope

**In scope**: TermRock quality-fix commits + regenerated fixtures + migration notes; lookbook alias removal; compatibility record; first annotated tag; branch protection + normal PR governance enablement; `jackin❯` repins with reviewed visual diffs; roadmap item + roadmap overview (`docs/content/docs/roadmap/index.mdx`) status updates; final merge-sync; PR #794 readiness.

**Out of scope**: crates.io publication (separate operator decision — [Decision 1](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)); new components; other-consumer work; `termrock-testing`; Windows; changing `jackin❯`'s dependency from `rev` to tag ([Decision 21](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): the tag is a marker, not a dependency-form change).

## Git workflow

- TermRock: still bootstrap direct-`main` (signed, buildable, green) **until** the final checkpoint; after tagging, enable protection — from then on, TermRock changes go through normal PRs.
- `jackin❯`: signed commits on `feature/shared-tui-extraction`, pushed immediately. Each quality-fix adoption is its own repin commit whose message names the TermRock change and summarizes the reviewed visual diff.

## Steps

### Step 1: Land each quality-backlog fix in TermRock (one reviewed change each)

For each backlog item, in its own commit (or tight series): fix the defect; regenerate affected SVGs/buffer fixtures via `termrock-lookbook render`; update the component docs page in the same commit (atomic source/story/SVG/docs rule); write a migration note (what visibly changes, why, how consumers adapt). Add the quality-tier tests: corrected display-width cases (combining marks, ZWJ emoji, CJK wide, regional indicators, zero-width, clipping) and the non-color cue assertions for focus/selection/error states.

**Verify per fix**: full TermRock gate sweep green; `check --dir docs/public/component-previews` green against regenerated fixtures; the fix's visual diff is exactly what its migration note describes (review rendered SVG diffs).

### Step 2: Adopt each fix in `jackin❯` via deliberate repins

For each TermRock quality checkpoint (batching several fixes into one repin is acceptable if each was separately reviewed upstream): bump `rev`, commit `Cargo.lock`, rerun full `cargo xtask ci`, review the `jackin❯`-side visual diff (updated product fixtures/goldens where surfaces render differently now), and record the repin + diff summary in `evidence/stage5-quality.md`.

**Verify per repin**: full workspace green; every fixture change traces to a named backlog item's migration note.

### Step 3: Remove the lookbook donor-flag aliases

Delete the temporary `--terminal`/`[out-dir]`/`--check <dir>` alias forms from `termrock-lookbook`, leaving the documented `terminal`/`list`/`render --out`/`check --dir` contract ([Decision 2](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Update TermRock CI/docs invocations accordingly.

**Verify**: `termrock-lookbook --check x` → usage error; subcommand forms green in CI.

### Step 4: Compatibility record and final evidence

Update `compatibility.toml` (both repos' copies — TermRock's authoritative, `jackin❯` evidence mirror): exact TermRock revision (post-quality), exact `jackin❯` implementation-branch commit that passed the full suite against it, Rust/ratatui-core/ratatui-crossterm/crossterm cell, macOS+Linux, reproduction commands and results ([Decision 21](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): the record names the unmerged-but-tested `jackin❯` commit). Publish the supported-terminal baseline + compatibility matrix in the TermRock README/catalog (roadmap acceptance bullet).

**Verify**: both files updated; TermRock docs deploy green.

### Step 5: First annotated tag, then governance switch

1. Preconditions ([Decision 21](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)): all backlog fixes in TermRock; `jackin❯` branch fully green against the exact candidate commit (Step 2's last repin); API report re-reviewed and committed (it becomes the semver baseline).
2. Ensure workspace version and tag agree; create `git tag -a v0.y.z` (pick from the crate version, e.g. continue the donor's `0.6.x` line or start `v0.1.0` — record the operator's choice; the dossier fixes the format `v0.y.z`, not the number) and push the tag; create the GitHub release with the migration notes ([ch. 07, "Release model"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)).
3. Wire `cargo-semver-checks` as a required candidate gate for changes after the tag.
4. Enable TermRock branch protection: protected `main`, required `rust-required` + `docs-required`, one PR per branch, docs-with-API-changes rule; record that direct-`main` bootstrap mode is over (update TermRock `CONTRIBUTING.md`).

**Verify**: `gh api repos/tailrocks/termrock/tags --jq '.[].name'` → `["v0.y.z"]`; `gh api repos/tailrocks/termrock/branches/main/protection` shows required checks; a test push to `main` is rejected.

### Step 6: Final merge-sync, roadmap retirement, PR readiness

1. `git fetch origin && git merge --no-ff origin/main -m "chore(merge): sync main into feature/shared-tui-extraction"`; rerun any evidence affected by upstream changes ([Decision 20](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)); full `cargo xtask ci`.
2. Retire the roadmap item in `docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx`: tick Stage 5 and the remaining execution-state boxes, set `**Status**` per the roadmap-overview table (completed work → the **Completed** section of `docs/content/docs/roadmap/index.mdx` — update both together per the docs discipline), keep the item as status + canonical-doc links (durable detail lives in the research dossier and TermRock catalog). Update the research dossier only where it states execution-pending facts that are now complete (e.g. index "execution not started" phrasing) — reconcile, don't rewrite history.
3. Verify the full acceptance checklists one final time: [ch. 04, "Acceptance checklist"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx) (17 boxes) and [ch. 08, "Program complete"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx) (6 boxes) — record each verdict in `evidence/stage5-quality.md`.
4. Docs gates: `bun run build`, `check:links:fresh`, `cargo xtask docs repo-links`, `cargo xtask roadmap audit`, `cargo xtask research check` — all green.
5. Mark PR #794 ready for operator merge review (do not merge it yourself unless the operator has instructed; the roadmap retires only in this PR's final commits).

**Verify**: all checklists PASS with evidence; PR #794 green end-to-end; index rows for plans 001–009 → DONE.

## Test plan

- Quality-tier test additions per component (Step 1) — the corrected-width corpus and non-color cue assertions from [ch. 09, quality tier](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx).
- Regenerated fixture suites green in both repos after each fix/repin.
- `cargo-semver-checks` dry run against the new tag (expect: no changes yet).

## Done criteria

Roadmap "Acceptance" section, verbatim, plus program-complete gates:

- [ ] Shared crate builds/tests without any Tailrocks product crate, without Tokio, and (by default) without Crossterm
- [ ] Every historical parity diff traces to a recorded quality-backlog item; every backlog fix landed post-parity with regenerated fixtures + migration notes **before** the first tag
- [ ] Lookbook CLI previews and verifies every public component; Fumadocs catalog renders the same generated SVG states
- [ ] `jackin❯` pins an immutable TermRock revision; supported-terminal baseline + compatibility matrix published
- [ ] First annotated tag exists = committed API baseline; semver comparisons start with subsequent candidates
- [ ] TermRock `main` protection enabled after the final bootstrap checkpoint, before roadmap closure
- [ ] Roadmap + canonical docs in both repositories state one owner per component/policy; roadmap item status updated in item + overview; all docs gates green
- [ ] No other Tailrocks product repository was checked out, changed, or used as a gate (attest in evidence)
- [ ] PR #794 fully green and marked ready; index rows → DONE

## STOP conditions

- A quality fix changes behavior beyond its migration note (scope creep into unrecorded territory — record a new backlog entry and re-review instead of widening the change).
- `jackin❯`'s post-repin diff shows a change no migration note explains.
- Tag preconditions unmet in any order (e.g. tagging before the last `jackin❯` green run against that exact commit).
- Branch-protection enablement would block a still-needed bootstrap push — sequence error; finish TermRock work first.
- The operator has not confirmed the version number for the first tag.

## Maintenance notes

- Post-roadmap follow-ups to file (do not implement): Capsule `Tabs`/`HintBar` convergence; new components under the two-consumer rule (trees, progress, data tables, command palettes, multi-line editors); other-consumer adoptions; `termrock-testing` on demand; sccache after ≥20 timing runs; benchmarks/fuzz targets per [ch. 07, "Scheduled hygiene"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx).
- Ratatui/Crossterm upgrades now follow [Decision 14](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): library-first, one pair per revision line, `jackin❯` repins as oracle.
- After merge, consider pruning `plans/shared-tui-extraction/` per the plans-directory policy (shipped plan bodies are removed after source audit; evidence may be worth retaining or relocating into the dossier).
