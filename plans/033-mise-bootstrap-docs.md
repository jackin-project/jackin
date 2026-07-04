# Plan 033: Make `mise install` the single documented bootstrap; stop ad hoc nextest installs

> **Executor instructions**: Docs/DX consistency fix. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- TESTING.md CONTRIBUTING.md README.md mise.toml`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P2
- **Effort**: S
- **Risk**: LOW (docs only)
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`TESTING.md` told contributors to install cargo-nextest directly with Cargo — which **directly contradicts**
the repo's own hard rule ("All tools — in CI and locally — must be installed through mise. Never … cargo
install", stated in `.github/AGENTS.md` and `crates/AGENTS.md`). `mise.toml` pins `cargo-nextest` to a
specific version; a `cargo install` pulls an unpinned newer one that can diverge from CI's pinned runner.
And **no doc tells a first-clone contributor to run `mise install` first**, even though nextest/audit/deny/
hack all come from mise — so a contributor either gets a mismatched nextest, or has no `cargo nextest` at all.

## Current state

- `TESTING.md:5-9` — direct cargo-nextest install.
- `.github/AGENTS.md` / `crates/AGENTS.md:117` — "Tools installed via `mise`, not ad-hoc `cargo install`".
- `mise.toml:9` — `cargo-nextest = "0.9.136"` (pinned).
- No "run `mise install` first" in `README.md`/`CONTRIBUTING.md`/`TESTING.md`.

## Scope

**In scope:** `TESTING.md`, `CONTRIBUTING.md`, `README.md` (bootstrap line). **Out of scope:** the mise
config itself; the nextest-only rule.

## Steps

### Step 1: Replace the direct cargo-nextest install instruction

In `TESTING.md:5-9`, replace the direct cargo-nextest install block with:
> Install the pinned toolchain and dev tools: `mise install` (installs `cargo-nextest`, `cargo-deny`,
> `cargo-audit`, etc. at the versions pinned in `mise.toml`). Do **not** `cargo install` these — CI uses the
> mise-pinned versions.

**Verify**: grep for the old direct cargo-nextest install in `TESTING.md` → no matches;
`grep -n "mise install" TESTING.md` → ≥1 match.

### Step 2: Add the first-clone bootstrap step

Add "run `mise install` first" to the onboarding path in `CONTRIBUTING.md` (and a one-liner in `README.md`'s
build-from-source pointer), before the merge-readiness/test commands, so a fresh clone has the tools.

**Verify**: `grep -rn "mise install" CONTRIBUTING.md README.md` → ≥1 match each (or CONTRIBUTING at minimum).

### Step 3: Sweep for other `cargo install` drift

`grep -rn "cargo install" *.md .github/*.md crates/*/*.md docs/*.md` — fix any other doc telling a
contributor to `cargo install` a mise-pinned tool.

## Done criteria

- [x] direct cargo-nextest install instructions removed (outside vendored `node_modules`/`target`)
- [x] `mise install` documented as the single bootstrap in `TESTING.md` + `CONTRIBUTING.md`
- [x] No remaining doc instructs `cargo install` of a mise-pinned tool
- [x] `plans/README.md` row updated

## Completion notes

- Replaced `TESTING.md` nextest installation with `mise install` and explained that mise pins the same tool
  versions as CI.
- Added first-clone `mise install` guidance to `CONTRIBUTING.md` and `README.md`.
- Updated the role-authoring Dockerfile example to install additional Rust tools through mise's Cargo
  backend instead of direct Cargo installs.
- Verification:
  - Repository grep for the old direct cargo-nextest install returns no matches outside this plan's
    historical notes.
  - `rg -n "mise install" README.md CONTRIBUTING.md TESTING.md` finds all three bootstrap docs.

## STOP conditions

- A tool the docs tell contributors to `cargo install` is genuinely **not** in `mise.toml` — then either add
  it to mise (small change, note it) or report; don't just delete the install instruction and leave the tool
  unobtainable.

## Maintenance notes

- Reviewer: any new tool a contributor needs must be pinned in `mise.toml`, never `cargo install`-ed in
  docs — this plan makes the docs match the existing hard rule.
