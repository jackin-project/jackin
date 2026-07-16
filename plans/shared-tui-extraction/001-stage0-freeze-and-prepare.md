# Plan 001: Freeze donor evidence and prepare execution (Stage 0)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: `git diff --stat 03928e9dd..HEAD -- crates/jackin-tui crates/jackin-tui-lookbook crates/jackin-core .github/workflows/ci.yml docs/content/docs/reference/tui docs/public/tui-lookbook`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

This plan executes **Stage 0** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) as specified by [ch. 04 — Extraction And Migration Plan, "Stage 0: Freeze and prepare execution"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx) and [ch. 08 — Migration Evidence And Gates, "Freeze artifacts"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx). Nothing may be extracted, filtered, or published until donor behavior is frozen as verifiable evidence, the time-sensitive external namespaces are revalidated, and the approved external-`main` write scope is surfaced. Every later parity claim (plans 007–009) is only as strong as the baseline this plan freezes.

## Current state

Facts verified on 2026-07-15 at `03928e9dd`:

- Branch `feature/shared-tui-extraction` exists with placeholder commit `03928e9dd feat: Shared TUI Extraction - branch placeholder`; PR #794 is open (`feat: Shared TUI Extraction - Extract TermRock from jackin❯`). The single-PR topology of [Decision 18](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx) is therefore already satisfied — do not create another branch or PR.
- Donor revision recorded by the dossier: `33896a504e19ef13adb8692550c1845cb86a9504`. Verified: it is an ancestor of HEAD and **zero commits** have touched `crates/jackin-tui` or `crates/jackin-tui-lookbook` since (`git log --oneline 33896a50..origin/main -- crates/jackin-tui crates/jackin-tui-lookbook` is empty). The donor has not drifted from the research baseline.
- `crates/jackin-tui/Cargo.toml` — `version = "0.6.0-dev"`, `publish = false`, depends on `jackin-core`, `ratatui`, `crossterm`, `tokio`, `tui-scrollbar`, `unicode-width`, `anstyle-parse`, `base64`, `owo-colors`, `similar`.
- The five `jackin_core` reference lines in the donor (measured with `rg -n jackin_core crates/jackin-tui --glob '*.rs'`):
  - `crates/jackin-tui/src/ansi.rs:14` — `pub use jackin_core::{POINTER_DEFAULT, POINTER_HAND};`
  - `crates/jackin-tui/src/ansi.rs:193` — `pub use jackin_core::encode_osc52_clipboard_write;`
  - `crates/jackin-tui/src/lib.rs:28` — `pub use jackin_core::shorten_home;`
  - `crates/jackin-tui/src/lib.rs:29-32` — `pub use jackin_core::{BOTTOM_CHROME_ROWS, BottomChromeAreas, DialogBodyScroll, StatusFooterHover, TailScroll, bottom_chrome_areas, is_scrollable, max_line_width, max_offset};`
  - `crates/jackin-tui/src/scroll.rs:20` — `pub use jackin_core::{TailScroll, is_scrollable, max_line_width, max_offset};`
- 195 Rust files outside the donor crates reference `jackin_tui` (re-verified: `rg -l 'jackin_tui' --glob '*.rs' | grep -v '^crates/jackin-tui' | wc -l` → 195).
- 29 committed SVG fixtures in `docs/public/tui-lookbook/`.
- Lookbook CLI usage (from `crates/jackin-tui-lookbook/src/main.rs:47-48`): `usage: tui-lookbook --terminal | tui-lookbook [out-dir] | tui-lookbook --check <dir>`. **Warning:** the CLI treats any first argument that is not `--terminal`/`--check` as an output directory and renders into it — always pass an explicit scratch dir.
- The lookbook `--check` drift gate is **not** wired into CI: `rg -n 'lookbook' .github/workflows/` matches nothing. The stable required aggregator job is `ci-required:` at `.github/workflows/ci.yml:1287` ("Single stable check name for branch-protection required-status").
- Stale donor docs to fix at freeze (per [ch. 02 — Donor Audit, "Known defects and stale artifacts"](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx)): `crates/jackin-tui/COMPONENTS.md` references a lookbook binary and in-crate story module that no longer exist; the crate-root docs in `crates/jackin-tui/src/lib.rs` name a `Theme` type that was never implemented.
- Toolchain facts for `compatibility.toml`: workspace `edition = "2024"`, `rust-version = "1.95"` (root `Cargo.toml:40-41`), pinned toolchain `1.97.0` (`rust-toolchain.toml`). Dependency cell per [Decision 7](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): `ratatui` 0.30.2, `ratatui-core` 0.1.2, `ratatui-crossterm` 0.1.2, `crossterm` 0.29.0.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Fast merge-readiness | `cargo xtask ci --fast` | exit 0 |
| Full merge-readiness | `cargo xtask ci` | exit 0 |
| Format | `cargo fmt --all -- --check` | exit 0 |
| Lookbook render | `cargo run -p jackin-tui-lookbook -- <scratch-dir>` | SVGs written to scratch dir |
| Lookbook drift check | `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` | exit 0, no drift |
| Donor tests | `cargo nextest run -p jackin-tui -p jackin-tui-lookbook` | all pass |
| Docs link gates | `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` | exit 0 each |

## Scope

**In scope** (files you may create or modify):

- `plans/shared-tui-extraction/evidence/` (create; all freeze artifacts)
- `.github/workflows/ci.yml` (add lookbook drift job wired into `ci-required`)
- `crates/jackin-tui/COMPONENTS.md`, `crates/jackin-tui/src/lib.rs` (stale-doc corrections only — no behavior change)
- PR #794 body (launch summary)

**Out of scope** (do NOT touch):

- Any rendering/behavior code change in the donor crates — Stage 0 freezes behavior; fixing recorded defects now would destroy the parity baseline (defects are Stage 5, plan 009).
- `tailrocks/termrock` — no external repository interaction whatsoever in this plan except read-only namespace checks.
- Any other Tailrocks repository (Holla, Velnor, Parallax, TableRock).

## Git workflow

- Branch: `feature/shared-tui-extraction` (already active; verify with `git branch --show-current`). Never switch or create branches.
- Every commit: `git commit -s -m "<type>(tui): …"` then `git push` immediately. Suggested subjects: `ci(tui): wire lookbook SVG drift check into ci-required`, `docs(tui): correct stale COMPONENTS.md and crate-root Theme reference`, `chore(tui): record shared-tui-extraction freeze evidence`.

## Steps

### Step 1: Confirm branch/PR state and donor freshness

Run `git branch --show-current` (expect `feature/shared-tui-extraction`), `gh pr list --head feature/shared-tui-extraction` (expect PR #794 open), and `git log --oneline 33896a504e19ef13adb8692550c1845cb86a9504..origin/main -- crates/jackin-tui crates/jackin-tui-lookbook` after `git fetch origin`.

**Verify**: last command prints nothing → donor unchanged; record `33896a504e19ef13adb8692550c1845cb86a9504` as the frozen donor revision. If it prints commits, the donor drifted: record the new `origin/main` revision as the frozen donor revision instead, merge-sync per [Decision 20](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx) (`git merge --no-ff origin/main -m "chore(merge): sync main into feature/shared-tui-extraction"`), and use that revision everywhere this plan says "frozen donor revision".

### Step 2: Revalidate external namespaces and record owners (time-sensitive gate)

Per roadmap checkbox "time-sensitive GitHub/crates.io namespaces … revalidated and recorded during Stage 0":

1. `gh repo view tailrocks/termrock` → expect "Could not resolve" (name free) **or** an empty repository owned by the operator's `tailrocks` org.
2. `cargo search termrock --limit 5` → expect no exact `termrock` match; also check `termrock-lookbook`.
3. Write results, check date, intended owner (`tailrocks` GitHub org; operator's crates.io account), and the operator's trademark disposition into `plans/shared-tui-extraction/evidence/namespaces.md`. The dossier records both names free as of 2026-07-15 ([Decision Record, "Naming"](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Trademark disposition is an operator statement — if you cannot obtain it, record "pending operator statement" and flag it in the PR; it must be resolved before plan 005's public push.

Also verify the history-preserving extraction tooling now (ch. 04 Stage 0: "verify the history-preserving extraction tool and attribution procedure"): install `git-filter-repo` (`pipx install git-filter-repo` or via mise), run `git filter-repo --version`, and record version + planned attribution procedure (authorship/timestamps/SPDX preserved; details in plan 002) in `namespaces.md` or a sibling note.

**Verify**: `test -s plans/shared-tui-extraction/evidence/namespaces.md` → exit 0; `git filter-repo --version` → prints a version. If either namespace is taken by a third party → STOP (operator must choose a fallback name; that is a new recorded decision).

### Step 3: Wire the lookbook SVG drift check into CI (freeze prerequisite)

Per [ch. 08, "Freeze artifacts"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx): the 29 committed fixtures are not mechanically proven current until `tui-lookbook --check` runs in CI, so they cannot serve as parity evidence.

1. In `.github/workflows/ci.yml`, add a job (suggested id `tui-lookbook-drift`) in the Rust lane that runs `cargo run --locked -p jackin-tui-lookbook -- --check docs/public/tui-lookbook`. Model its structure (runner selection, toolchain cache steps, timeout, permissions) on the existing `cargo clippy` job around `ci.yml:511`; gate it on the same Rust path-classification condition the other Rust jobs use.
2. Add the new job to the `needs:` list of the `ci-required` aggregator at `ci.yml:1287` so it becomes durably required without a branch-protection change.
3. Run the check locally first: `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` → exit 0. If it reports drift → STOP condition (fixtures are stale; regenerating them changes the baseline and needs operator review).

**Verify**: `actionlint .github/workflows/ci.yml` → exit 0 (tool available via `mise install`). Commit (`ci(tui): …` with `-s`), push, and confirm the new job appears and passes on PR #794's checks (`gh pr checks 794 --watch`).

### Step 4: Fix stale donor docs (docs-only, no behavior change)

1. `crates/jackin-tui/COMPONENTS.md`: remove/correct references to the removed lookbook binary and in-crate story module; point at `crates/jackin-tui-lookbook` and the `tui-lookbook` CLI usage quoted in "Current state".
2. `crates/jackin-tui/src/lib.rs` crate-root doc comment: remove the claim that a `Theme` type exists (it was never implemented; the palette is constants — see [ch. 06, "Theme API"](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx)). Doc-comment text only — do not rename or add any Rust item.

**Verify**: `cargo doc -p jackin-tui --no-deps` → exit 0; `git diff --stat` shows only the two files; `cargo nextest run -p jackin-tui` → all pass (proves no code change). Commit + push.

### Step 5: Generate the freeze artifacts

Create `plans/shared-tui-extraction/evidence/` and generate each artifact from [ch. 08's freeze-artifact table](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx). Record the exact command used at the top of each file as a comment. Baseline commands (run from repo root, adjust only if a tool is unavailable):

1. `public-api.txt` — donor public surface: `rg -n '^\s*pub (fn|struct|enum|trait|type|const|static|use|mod|macro_rules)' crates/jackin-tui/src crates/jackin-tui-lookbook/src > plans/shared-tui-extraction/evidence/public-api.txt`. If `cargo public-api` is installed, append its output for `jackin-tui` as a second section.
2. `consumer-map.tsv` — file, crate, imported items: for each of the 195 files from `rg -l 'jackin_tui' --glob '*.rs' | grep -v '^crates/jackin-tui'`, emit `path<TAB>workspace-package<TAB>imported-item` rows using `rg -o 'use jackin_tui::[^;]+' <file>`. Group counts must reproduce the ch. 08 table (105 `jackin-console`, 42 `jackin-capsule`, 26 `jackin-launch-tui`, 11 root `jackin`, 4 `jackin-console-oppicker`, 2 `jackin-runtime`, 2 `jackin-diagnostics`, 2 `jackin-core`, 1 `jackin-host`). Then add a second section for the **`jackin-core` compatibility helpers** (ch. 04 Stage 0 requires identifying every consumer of both): `rg -ln 'TailScroll|is_scrollable|max_line_width|max_offset|DialogBodyScroll|StatusFooterHover|BOTTOM_CHROME_ROWS|BottomChromeAreas|bottom_chrome_areas|encode_osc52_clipboard_write|POINTER_DEFAULT|POINTER_HAND' --glob '*.rs'` — every hit tagged with whether it resolves through `jackin_tui` re-exports or `jackin_core` directly (plan 007 slice 1 migrates both populations and deletes the duplicates).
3. `extraction-ledger.csv` — one row per public item/module with columns `item,current_path,decision,target_owner,dependencies,tests,docs,notes`. Populate `decision`/`target_owner` verbatim from [ch. 09 — Component Redesign Catalog](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx) ("Foundation module decisions", "Component-by-component disposition" tables) and [ch. 06's donor refactoring ledger](../../docs/content/docs/reference/research/shared-tui-extraction/06-public-api-and-refactoring.mdx). Every row must have a decision: extract | parameterize | remain | remove. No unclassified item may remain.
4. `story-manifest.json` — for every lookbook story in `crates/jackin-tui-lookbook/src/stories.rs`: story ID, component, dimensions, fixture SVG filename, and a `product_terms` boolean (true where the story names agents/roles/workspaces/containers/product paths — those stay local per [ch. 02, "Keep in jackin❯"](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx)).
5. `render-manifest.json` — `shasum -a 256 docs/public/tui-lookbook/*.svg` plus SVG pixel dimensions; 29 entries expected.
6. `dependency-tree.txt` — `cargo tree -p jackin-tui` and `cargo tree -p jackin-tui-lookbook` (normal + `--all-features`).
7. `compatibility.toml` — the initial tested cell from "Current state": Rust 1.97.0 toolchain / 1.95 floor / edition 2024, `ratatui` 0.30.2, `ratatui-core` 0.1.2, `ratatui-crossterm` 0.1.2, `crossterm` 0.29.0, Linux + macOS, `jackin❯` frozen donor revision.
8. `quality-backlog.md` — the bug-compatible defect list from [ch. 02, "Known defects"](../../docs/content/docs/reference/research/shared-tui-extraction/02-donor-audit.mdx) and [Decision 13](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx), each with a post-parity fix note: (a) character-count width math in `HintSpan::display_cols`, select-list label measurement, status-footer right group, error-dialog wrap math; (b) color-only panel focus border (no non-color cue); (c) three parallel color layers plus the duplicated hex table in the lookbook SVG writer; (d) hyperlink overlays returned as raw post-frame byte vectors from error/container-info dialogs. State explicitly: none of these may be fixed before `jackin❯` parity passes.
9. `performance-baseline.md` — the [ch. 08 "Performance gates"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx) baseline, recorded **before any data-ownership change**: clean default and all-feature compile time for the two donor crates (`cargo build -p jackin-tui --timings`, repeat `--all-features`); time-to-first-interactive-lookbook-frame (coarse wall clock of `tui-lookbook --terminal` startup is acceptable — record the method); render time/allocations for tabs, 10/1,000/100,000-row list projections, long Unicode labels, and large diffs (use existing benches if present, else a small `#[test]`-gated timing harness committed under `evidence/` — record method and machine); SVG catalog generation time and output byte size (`time cargo run -p jackin-tui-lookbook -- /tmp/claude-lookbook-perf`); terminal restore latency observations. Plans 006 and 009 re-measure against this file: advisory during neutralization, budgeted after parity.

**Verify**: all nine files exist and are non-empty (`ls -l plans/shared-tui-extraction/evidence/`); consumer-map group counts match the table above; every extraction-ledger row has a decision. Commit + push (`chore(tui): record shared-tui-extraction freeze evidence`).

### Step 6: Capture the parity baseline

1. Run the full donor test suite: `cargo nextest run -p jackin-tui -p jackin-tui-lookbook` → all pass; record the pass count in `evidence/parity-baseline.md`.
2. Render to a scratch dir and confirm byte-identity with committed fixtures: `cargo run -p jackin-tui-lookbook -- /tmp/claude-lookbook-freeze && diff -r /tmp/claude-lookbook-freeze docs/public/tui-lookbook` → no differences (also proves the Step 3 CI job will stay green).
3. Record in `evidence/parity-baseline.md`: nextest counts, SVG hash-set reference (points at `render-manifest.json`), and the frozen donor revision.

**Verify**: `test -s plans/shared-tui-extraction/evidence/parity-baseline.md`. Commit + push.

### Step 7: Surface the launch summary in PR #794

Edit the PR body (`gh pr edit 794 --body-file …`) to state, per roadmap Stage 0 and [Decision 18](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx): (a) this program creates `tailrocks/termrock` if absent; (b) bootstrap commits push **directly to TermRock `main`** without TermRock PRs until the final tag; (c) the frozen donor revision; (d) links to the roadmap item (`/roadmap/shared-tui-extraction/`), the research dossier (`/reference/research/shared-tui-extraction/`), and `plans/shared-tui-extraction/`. Keep whatever body content already exists; append a "Launch summary" section. PR prose is a rich-text surface: write `jackin❯`.

**Verify**: `gh pr view 794 --json body | grep -c 'tailrocks/termrock'` ≥ 1.

## Test plan

No new Rust tests in this plan (Stage 0 freezes behavior). The verification surface is: the new CI job passing on PR #794, the byte-identical scratch render in Step 6, and the unchanged donor test suite after the docs-only edits in Step 4.

## Done criteria

Stage 0 exit gate ([ch. 04](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx)): no ownership ambiguity; single PR open; freeze evidence reproducible; external name/owners/write-scope recorded. Concretely, ALL must hold:

- [ ] `gh pr checks 794` shows the lookbook drift job green inside `ci-required`
- [ ] All nine evidence files plus `namespaces.md` and `parity-baseline.md` committed under `plans/shared-tui-extraction/evidence/`
- [ ] `extraction-ledger.csv` has zero rows with an empty `decision` column
- [ ] `cargo nextest run -p jackin-tui -p jackin-tui-lookbook` → all pass, unchanged counts
- [ ] `git status` clean; every commit signed (`git log --format='%(trailers:key=Signed-off-by,only)' 03928e9dd..HEAD` non-empty per commit) and pushed
- [ ] PR #794 body contains the launch summary
- [ ] Roadmap Stage 0 checkbox NOT yet ticked in `docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx` — tick it in this plan's final commit only after every box above holds (`docs(roadmap): mark shared-tui-extraction stage 0 complete`), and in the same commit tick the two "Decision and execution state" boxes now true: namespace/owner/trademark revalidation and "Implementation has started"
- [ ] `plans/shared-tui-extraction/README.md` status row → DONE

## STOP conditions

- `tui-lookbook --check` reports drift against committed fixtures (stale baseline — operator must review before anything is frozen).
- Either external namespace is taken by a third party (Step 2).
- Donor code changed on `origin/main` in a way that contradicts the "Current state" excerpts after merge-sync.
- You find yourself editing donor `.rs` behavior (anything beyond the lib.rs doc comment) — that is Stage 2+ work in the wrong repository.
- A security defect, data-loss path, or panic on valid input is discovered — report; the operator decides the Stage 0 donor fix before re-freezing ([Decision 13](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)).

## Maintenance notes

- The frozen donor revision and evidence files are inputs to every later plan; regenerating any of them after Stage 0 requires the re-freeze protocol, not an in-place edit.
- Reviewers should scrutinize: the CI job wiring (must be inside `ci-required`, not a floating optional job) and that Step 4 changed zero behavior.
- Deferred by design: all rendering defect fixes (plan 009); namespace *creation* (plan 002).
