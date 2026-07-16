# Plan 002: Prepare the filtered external donor history (Stage 1)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If anything in "STOP conditions" occurs, stop and report — do not improvise. When done, update this plan's row in `plans/shared-tui-extraction/README.md`.
>
> **Drift check (run first)**: confirm plan 001 is DONE in the index and `plans/shared-tui-extraction/evidence/` exists with all freeze artifacts. Then `git diff --stat 03928e9dd..HEAD -- crates/jackin-tui crates/jackin-tui-lookbook` — donor changes beyond plan 001's docs-only edits are a STOP condition unless a re-freeze was recorded.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (public repository creation; irreversible if pushed wrongly — mitigated by keeping everything local)
- **Depends on**: plans/shared-tui-extraction/001-stage0-freeze-and-prepare.md
- **Category**: migration
- **Planned at**: commit `03928e9dd`, 2026-07-15

## Why this matters

This plan executes **Stage 1** of the [Shared TUI Extraction roadmap item](../../docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx) per [ch. 04, "Stage 1: Prepare the external donor history"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx) and [ch. 08, "History extraction"](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx). TermRock must carry path-level donor provenance (authorship, SPDX, Apache attribution) without ever exposing an unbuildable or unscanned donor tip as a public head. Everything in this plan happens in a **dedicated local extraction clone**; the only external action is creating the *empty* repository. The filtered history is the foundation plans 003–005 commit on top of.

## Current state

- Plan 001 recorded the frozen donor revision (expected `33896a504e19ef13adb8692550c1845cb86a9504`) and committed the freeze evidence.
- Donor paths to retain with full history ([Decision 5](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)): `crates/jackin-tui/`, `crates/jackin-tui-lookbook/`, plus exact path-complete neutral docs and assets: the generic lookbook pages under `docs/content/docs/reference/tui/lookbook/`, committed previews `docs/public/tui-lookbook/`, the relevant neutral TUI design files under `docs/content/docs/reference/tui/` (per the plan-001 extraction ledger's docs column), and root `LICENSE`/`NOTICE`.
- Mixed `jackin-core` files are **not** filtered ([Decision 5](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)): the neutral helpers re-exported at `crates/jackin-tui/src/lib.rs:28-32`, `scroll.rs:20`, `ansi.rs:14`, `ansi.rs:193` are reimplemented later (plan 003) as new DCO-signed TermRock commits with lineage recorded in `provenance.toml`.
- The repository uses SPDX headers per file (e.g. `crates/jackin-tui/Cargo.toml:1-2`: `# SPDX-FileCopyrightText: 2026 Alexey Zhokhov` / `# SPDX-License-Identifier: Apache-2.0`) and a repository-wide `REUSE.toml` that must NOT be copied unchanged ([ch. 07, dependency and provenance gates](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)).
- Tooling: [`git-filter-repo`](https://github.com/newren/git-filter-repo) is the decided tool. Install into the pinned toolchain (`mise use -g git-filter-repo` or `pipx install git-filter-repo`); verify with `git filter-repo --version`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Dedicated clone | `git clone --no-local <jackin-origin-url> ~/termrock-extraction` | fresh clone, NOT the active workspace |
| History filter | `git filter-repo --path crates/jackin-tui --path crates/jackin-tui-lookbook --path docs/content/docs/reference/tui/lookbook --path docs/public/tui-lookbook --path LICENSE --path NOTICE …` | filtered history, only retained paths |
| Secret scan | `gitleaks git --redact .` (or `trufflehog git file://.`) | zero findings |
| Size scan | `git rev-list --objects --all \| git cat-file --batch-check='%(objecttype) %(objectname) %(objectsize) %(rest)' \| sort -k3 -n -r \| head -20` | no unexpected oversized binaries |
| Author audit | `git log --format='%an %ae' \| sort -u` | expected donor authors only |
| Empty repo creation | `gh repo create tailrocks/termrock --public` | repo exists, zero commits |

## Scope

**In scope**:

- A new dedicated clone directory outside the active workspace (e.g. `~/termrock-extraction`) — all filtering happens there.
- Creating the empty public `tailrocks/termrock` repository (no README, no license seed, no branch, no tag).
- `provenance.toml` (created in the extraction clone, committed in plan 003's first bootstrap commit).
- `plans/shared-tui-extraction/evidence/stage1-history-review.md` in `jackin❯` (scan results, filter command, boundary commit).

**Out of scope** (do NOT touch):

- The active `jackin❯` working tree's donor crates — never run `git filter-repo` in the active workspace ([ch. 08](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx) is explicit).
- **Pushing any source to `tailrocks/termrock`** — the first push happens at the end of plan 005, after neutralization. Stage 1 publishes nothing.
- Donor branches/tags — never pushed, ever.
- Any file from TablePro, TablePlus, Zedis, or other reference projects.

## Git workflow

- `jackin❯` side: evidence commit on `feature/shared-tui-extraction`, signed (`git commit -s`), pushed immediately. Suggested subject: `chore(tui): record stage 1 filtered-history review evidence`.
- Extraction clone side: the filtered history is inherited provenance — do NOT rebase, squash, amend, or retroactively DCO-sign inherited commits ([Decision 18](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Record the boundary; new commits start in plan 003.

## Steps

### Step 1: Create the dedicated extraction clone at the frozen revision

```sh
git clone --no-local https://github.com/jackin-project/jackin.git ~/termrock-extraction
cd ~/termrock-extraction
git checkout <frozen-donor-revision>   # from plan 001 evidence
```

**Verify**: `git -C ~/termrock-extraction rev-parse HEAD` prints the frozen donor revision; `pwd` is not the active workspace.

### Step 2: Determine the exact retained-path list

Start from the "Current state" path list. Cross-check against plan 001's `extraction-ledger.csv` docs column: every neutral component doc page and preview asset marked `extract` must be path-covered; every product story/page marked `remain` must NOT be. Where a directory mixes neutral and product files (e.g. `docs/content/docs/reference/tui/`), enumerate exact file paths instead of the directory.

**Verify**: write the final list into the filter command in Step 3 and into `stage1-history-review.md`. Cross-check both directions: `for` each ledger row `decision=extract` with a docs/assets path → present in list; each `remain` path → absent.

### Step 3: Run git-filter-repo and record the exact command

In the clone, run `git filter-repo` with one `--path` per retained entry (directories and exact files from Step 2). Record the **complete literal command** in `stage1-history-review.md` and later in `provenance.toml`. Do not use `--path-rename` yet; the path/reorganization map (donor layout → target `crates/termrock/src/{text,input,interaction,layout,scroll,style,osc,runtime,widgets,crossterm}/` layout from [ch. 09, "Target module tree"](../../docs/content/docs/reference/research/shared-tui-extraction/09-component-redesign-catalog.mdx)) is *prepared as a document* now and *executed as ordinary signed move commits* in plans 003–004, so file moves stay reviewable ([ch. 04 Stage 1: "prepare the path/reorganization map without committing a partial new TermRock bootstrap"](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx)).

**Verify**: `git -C ~/termrock-extraction ls-files | grep -v -E '^(crates/jackin-tui|crates/jackin-tui-lookbook|docs/content/docs/reference/tui|docs/public/tui-lookbook|LICENSE|NOTICE)'` → empty (nothing outside retained paths); `git log --oneline | wc -l` > 1 (history preserved, not squashed).

### Step 4: Audit the filtered history

1. **Authors/timestamps**: `git log --format='%an %ae %ad' | sort -u` — expected donor authors only; timestamps preserved.
2. **Secrets**: `gitleaks git --redact .` over the full history → zero findings. If a finding appears, do NOT copy the value anywhere; record file/commit/credential-type only and STOP (operator must decide redaction + rotation).
3. **Oversized binaries**: object-size scan from the commands table → nothing unexpected beyond SVG/PNG assets.
4. **License**: every retained file has an SPDX header or is covered by attribution; root `LICENSE` (Apache-2.0) and `NOTICE` history retained.
5. **Reference-project source**: `rg -il 'tablepro|tableplus|zedis' ~/termrock-extraction` → no matches ([ch. 08](../../docs/content/docs/reference/research/shared-tui-extraction/08-migration-evidence-and-gates.mdx): no reference-project source).

**Verify**: all four scans clean; results recorded in `stage1-history-review.md`.

### Step 5: Record the provenance boundary and draft `provenance.toml`

Record `git rev-parse HEAD` of the filtered tip as the **imported-history boundary**: every commit ≤ boundary is inherited provenance (no DCO, may not build standalone); every commit after it must be DCO-signed and buildable ([Decision 18](../../docs/content/docs/reference/research/shared-tui-extraction/05-decision-record.mdx)). Draft `provenance.toml` in the clone root with: donor repository URL, frozen donor revision, retained path list, the literal filter command, the boundary commit, the license decision (Apache-2.0, preserved attribution), and an empty `[[reimplemented]]` table array to be filled by plan 003 for each `jackin-core` helper (source file, donor revision, meaningful source commit).

**Verify**: `provenance.toml` parses (`python3 -c "import tomllib,sys;tomllib.load(open(sys.argv[1],'rb'))" provenance.toml` → exit 0).

### Step 6: Create the empty public repository

Confirm plan 001's `namespaces.md` records the name free and the trademark disposition resolved (or explicitly accepted-pending by the operator). Then:

```sh
gh repo create tailrocks/termrock --public --description "Ratatui components, lookbook, and documentation for Tailrocks applications"
```

Do NOT pass `--add-readme`, license, or gitignore flags — GitHub must not generate a seed commit ([ch. 07, "Initial repository"](../../docs/content/docs/reference/research/shared-tui-extraction/07-repository-engineering.mdx)). Add the remote in the clone (`git remote add termrock https://github.com/tailrocks/termrock.git`) but **do not push**.

**Verify**: `gh api repos/tailrocks/termrock --jq '.size'` → `0`; `gh api repos/tailrocks/termrock/branches --jq 'length'` → `0`.

### Step 7: Commit the `jackin❯`-side evidence

Write `plans/shared-tui-extraction/evidence/stage1-history-review.md` in the active workspace: filter command, retained paths, boundary commit, scan results (secrets/size/authors/license/reference-source), extraction-clone location, and the repo-creation record. Commit + push on `feature/shared-tui-extraction`.

**Verify**: file committed; `git status` clean.

## Test plan

No Rust tests here — the verification surface is the history audit itself (Step 4's four clean scans) plus the two `gh api` assertions that the public repository is empty. The filtered tip is expected to NOT build standalone; do not "fix" that now (plan 003 does).

## Done criteria

Stage 1 exit gate ([ch. 04](../../docs/content/docs/reference/research/shared-tui-extraction/04-extraction-migration-plan.mdx)): provenance traces extracted code to donor commits; pre-publish history/license/secret review passes; the clone is ready for neutralization; no raw donor tip published. Concretely:

- [ ] `~/termrock-extraction` contains only retained paths, full history, checked out at the filtered tip
- [ ] `stage1-history-review.md` committed in `jackin❯` with the literal filter command and all scan results
- [ ] `provenance.toml` drafted in the clone, boundary commit recorded
- [ ] `tailrocks/termrock` exists, public, **zero commits, zero branches**
- [ ] No push of any kind to `tailrocks/termrock`
- [ ] Roadmap Stage 1 checkbox ticked in `docs/content/docs/roadmap/(operator-surface)/shared-tui-extraction.mdx` in the same evidence commit
- [ ] `plans/shared-tui-extraction/README.md` status row → DONE

## STOP conditions

- Any secret-scan finding (record location + type only — never the value; operator decides redaction/rotation before proceeding).
- An incompatible license or reference-project source appears in the filtered history.
- The namespace was claimed between plan 001 and Step 6.
- You are about to run `git filter-repo` in the active `jackin❯` workspace, or `git push` in the extraction clone — both are prohibited here.
- The trademark disposition is still "pending operator statement" and the operator has not accepted proceeding.

## Maintenance notes

- The extraction clone is a long-lived working area through plan 005 — do not delete it between plans; record its path in the evidence file.
- Reviewers should scrutinize the retained-path list against the extraction ledger — a missed neutral doc page is cheap to fix now and expensive after plan 005's publish.
- If the filter must be re-run (wrong path list), start again from Step 1 with a fresh clone; never iterate filter-repo on an already-filtered clone without recording each command.
