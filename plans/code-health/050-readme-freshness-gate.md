# Plan 050: README-freshness gate — structural crate changes must touch the crate README in the same PR

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-xtask/src .github/workflows/ci.yml`
> Plans 011/022/036 touch ci.rs/ci.yml — expected drift; add beside. New
> xtask lint modules since the excerpt: read them as additional exemplars.

## Status

- **Priority**: P2
- **Effort**: S-M
- **Risk**: LOW-MED (heuristic gate — false positives are the risk; mitigated by advisory-first + module-level-changes-only trigger)
- **Depends on**: none (plan 049's pipeline is NOT needed — this gate reads only the git diff; plan 015's presence gate is complementary, not a dependency)
- **Category**: dx / docs
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

`crates/AGENTS.md` makes it a hard rule: "Update the README in the same PR whenever you change … the `src/` module layout (add/rename/split/remove a module or subdirectory)." Nothing enforces it — plan 015 gates README *presence*, not freshness — and the drift class is real: the capsule README stub and the stale codebase map were both cleanup items (plan 029). The roadmap (Phase 5 item 8) asks for "a cheap heuristic check that a crate whose `src/` module layout changed in a PR also touched its README." The old index parked this behind the extraction pipeline; the gate needs only `git diff --name-status`, so it is plannable now. The roadmap's ratchet principle applies: advisory first, promote when the false-positive rate is known.

## Current state

Verified at `fabe88406`.

- The rule: `crates/AGENTS.md` "Per-crate README + AGENTS.md (hard rule)" section — README updates required for responsibility/API/layout/tier/verification changes; "Line-count churn inside an existing module does not require a README edit." The gate therefore triggers ONLY on `.rs` file adds/renames/deletes under `crates/<x>/src/`, never on content edits — exactly the "structural" line the rule draws.
- xtask gate exemplars: `crates/jackin-xtask/src/` has 13 gate modules (`lint.rs` file-size gate 11.2K with `bail!`-based messages; `test_layout.rs`; `agent_files.rs`; `arch.rs`; each with sibling `<mod>/tests.rs`). `lint.rs:210` shows the message style (`bail!` with the violating path + the fix). The repo's "diagnostics are prompts" principle: failure text must state the rule, why, the clearing edit, and the narrowest rerun command.
- CI wiring exemplar: gates run as jobs/steps in `.github/workflows/ci.yml` and locally via `cargo xtask ci` (`crates/jackin-xtask/src/ci.rs`). A PR-diff-aware gate needs the base ref: GitHub provides `GITHUB_BASE_REF` on pull_request events; locally the merge-base with `origin/main` serves.
- Diff mechanics: `git diff --name-status <base>...HEAD` (three-dot: changes on the PR side only). Statuses A/D/R on `crates/<x>/src/**/*.rs` = structural; M = not. README touch = any status on `crates/<x>/README.md`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| Run the gate locally | `cargo xtask lint readme-freshness --base origin/main` | pass/fail with named crates |
| Workspace clippy | `cargo clippy -p jackin-xtask --all-targets -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-xtask/src/readme_freshness.rs` (new) + `crates/jackin-xtask/src/readme_freshness/tests.rs`
- `crates/jackin-xtask/src/main.rs` (subcommand registration — read how `lint`'s subcommands dispatch and match)
- `.github/workflows/ci.yml` — one advisory step/job on pull_request (`continue-on-error: true` initially)
- `TESTING.md` gate row; `crates/jackin-xtask/README.md` structure row
- Roadmap Phase 5 item 8 status

**Out of scope** (do NOT touch):
- `cargo xtask ci` blocking composition (advisory phase first; promotion is a one-line follow-up recorded in maintenance).
- Plan 015's presence gate; plan 049's pipeline.
- Any README content itself.
- AGENTS.md freshness (READMEs only — AGENTS files are non-derivable rules that structural changes usually don't invalidate).

## Git workflow

- Branch off `main`: `feature/readme-freshness-gate`.
- Conventional Commits (`feat(xtask): …`, `ci: …`), `-s`, push per commit. PR to `main`; do not merge.

## Steps

### Step 1: The gate logic

`readme_freshness.rs`: `pub fn check(base: &str) -> Result<()>`:
1. Resolve the diff range: `git merge-base <base> HEAD` then `git diff --name-status <merge-base> HEAD` (run via the std process helper other xtask gates use — grep `Command` usage in `lint.rs`/`arch.rs` and copy the pattern; xtask is a non-render context, the disallowed-methods carve-out applies with an `#[expect(..., reason = …)]` only if the existing gates carry one — match them exactly).
2. Bucket paths: for each `crates/<x>/src/**/*.rs` with status A, D, or R* → mark crate `<x>` structural. Collect crates whose `crates/<x>/README.md` appears in the diff (any status).
3. Violations = structural crates minus README-touched crates, minus an allowlist param for generated/excluded crates (none initially; keep the hook).
4. Failure output (diagnostics-are-prompts): per crate — the rule, the triggering paths (up to 5), the fix ("update crates/<x>/README.md — structure table and/or public API section — in this PR"), and the rerun command (`cargo xtask lint readme-freshness --base origin/main`).

Tests (pure — factor the bucketing to take the parsed name-status list, no git): structural add triggers; M-only does not; rename triggers; README-touched clears; tests/`.rs`-outside-src do not trigger (decide: `src/**` only — `tests/` layout changes are covered by the test-layout gate).

**Verify**: `cargo nextest run -p jackin-xtask` → new tests pass.

### Step 2: Wire the subcommand

Register `lint readme-freshness` (or `readme-freshness` at top level — mirror where `arch`/`test-layout` sit; read main.rs and match the naming style) with `--base <ref>` defaulting to `origin/main`.

**Verify**: `cargo xtask lint readme-freshness --base origin/main` on this branch → PASSES only if this plan's own xtask structural change touched `crates/jackin-xtask/README.md` — which Step 4 does; run again after Step 4 to confirm the self-test.

### Step 3: Advisory CI step

Add to ci.yml, in an existing cheap gates job if one runs xtask lints on pull_request (read ci.yml for the job that runs `cargo xtask lint …`; append there rather than paying a new job's compile), the step: run the gate with `--base "origin/${GITHUB_BASE_REF:-main}"` after a `git fetch origin "$GITHUB_BASE_REF"`. `continue-on-error: true` + a step-summary line so the signal is visible without blocking.

**Verify**: push; the PR's CI shows the step green (this PR touches xtask src AND its README, so it self-passes); paste the step log line in the PR body.

### Step 4: Docs

`crates/jackin-xtask/README.md` structure row (self-satisfying the gate), TESTING.md row, roadmap item 8 note ("gate shipped advisory; pipeline-based content checks remain future work").

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Pure bucketing tests per Step 1 (5 cases minimum), following an existing xtask gate's `tests.rs` structure.
- The self-referential live check (Steps 2/3): this PR is itself a structural xtask change — the gate must demand and find this PR's README row.

## Done criteria

- [ ] `cargo xtask lint readme-freshness --base origin/main` exists; failure text names rule/paths/fix/rerun
- [ ] Pure tests cover A/D/R/M/README-touched/non-src cases
- [ ] Advisory CI step green on this PR (self-test)
- [ ] xtask README + TESTING.md + roadmap updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- CI's checkout depth prevents `git merge-base` against the base ref (shallow clone) and the existing workflow has no fetch-depth precedent to copy — report how other diff-aware steps (if any) handle it rather than flipping global fetch-depth.
- The xtask process-helper pattern turns out to route through a shared module another in-flight plan (036) is rewriting — coordinate: use whatever helper exists at HEAD, note the 036 rebase in the PR body.
- The false-positive story collapses (e.g. the repo routinely adds `src/**` files that genuinely need no README change beyond the structure table — if the structure table IS derivable-generated somewhere by the time this executes, the gate may be obsolete; report instead of shipping a dead gate).

## Maintenance notes

- Promotion to blocking: after ~2 weeks advisory, flip `continue-on-error` and add the gate to `cargo xtask ci`'s lint set — one-line follow-up, record the flip in the README index.
- Plan 049's generated pages make stale READMEs *visible*; this gate makes them *blocking*. Together they retire the drift class 029 cleaned up.
- Reviewer scrutiny: the A/D/R-only trigger (an M-triggering gate would spam every PR); rename detection requires `--find-renames` behavior — confirm `--name-status` default rename detection in the git version CI uses, or pass `-M` explicitly.
