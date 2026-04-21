# Conventional Commits + Franchise-Vocabulary Scrub Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adopt Conventional Commits as the documented and backfilled commit-message convention across all eight repos in `/Users/donbeave/Projects/jackin-project/`, and complete the franchise-vocabulary scrub that commit `ff2a859` left incomplete.

**Architecture:** Two phases. **Phase 1 (forward-only)**: open one PR per repo that adds a `## Commit Messages` section to `AGENTS.md` and (where applicable) finishes the franchise-vocab scrub in the working tree. Non-destructive, fully reversible. **Phase 2 (history rewrite, gated per repo)**: after Phase 1 PRs merge and the user confirms sibling agents are paused, rewrite non-conforming historical commits using `git filter-repo` (for `jackin`, large mapping) or interactive rebase (for the small repos), tag the pre-rewrite SHA, then `git push --force-with-lease`.

**Tech Stack:** `git`, `git-filter-repo` (Python tool, installed via `pip3 install --user git-filter-repo`), `gh` CLI, `cargo nextest`, `bun`, `tofu fmt`, plus existing repo verification commands per `AGENTS.md`.

**Spec:** `docs/superpowers/specs/2026-04-21-conv-commits-and-franchise-scrub-design.md`

---

## Prerequisites

Before starting any task:

- All eight repos cloned at `/Users/donbeave/Projects/jackin-project/<repo>/`.
- `gh` CLI authenticated for `github.com/jackin-project/*`.
- `mise trust` run inside `jackin/` so `cargo`/`bun` resolve from the local `mise.toml`.
- For Phase 2 only: `git-filter-repo` installed (Task 9 installs it).
- For Phase 2 only: user has confirmed sibling agents are paused.

## Canonical AGENTS.md `## Commit Messages` section

Used verbatim in every repo's AGENTS.md. Insert after the existing `## Branching` section if present, otherwise after `## Rules`.

```markdown
## Commit Messages

All commits in this repository MUST follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

Subject format: `<type>[optional scope][!]: <description>`

Allowed types:

| Type       | Use for                                                |
| ---------- | ------------------------------------------------------ |
| `feat`     | New user-visible feature                               |
| `fix`      | Bug fix                                                |
| `docs`     | Documentation-only change                              |
| `style`    | Formatting, whitespace; no logic change                |
| `refactor` | Internal restructuring; no behavior change             |
| `perf`     | Performance improvement                                |
| `test`     | Adding or updating tests                               |
| `build`    | Build system, tooling, dependencies                    |
| `ci`       | CI configuration                                       |
| `chore`    | Routine maintenance (release, merge, deps)             |
| `revert`   | Reverts a prior commit                                 |

Scope is optional but encouraged when it clarifies the change area, e.g., `feat(launch): preview resolved mounts per agent in TUI`.

Breaking changes use `!` after the type/scope (`feat!:` or `feat(api)!:`) and include a `BREAKING CHANGE:` footer in the body.

PR squash-merge: the PR title becomes the commit subject, so PR titles must also follow this convention.
```

---

# Phase 1 — Forward-only PRs (Tasks 1–8)

Each task creates a feature branch, edits files, verifies, commits, pushes, and opens a PR. Non-destructive. PRs can be opened in parallel — they are independent. Each task ends at "PR opened" — merging is the user's call.

---

### Task 1: `jackin` — Conv Commits doc + rainEngine label scrub

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/jackin/AGENTS.md`
- Modify: `/Users/donbeave/Projects/jackin-project/jackin/docs/src/components/landing/rainEngine.ts:10,12,13`
- Modify: `/Users/donbeave/Projects/jackin-project/jackin/docs/src/components/landing/rainEngine.test.ts:14`

**Branch:** `docs/conv-commits-and-finish-franchise-scrub`

- [ ] **Step 1: Verify current state of rainEngine.ts comment labels (failing "test")**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin
grep -n 'MATRIX_' docs/src/components/landing/rainEngine.ts docs/src/components/landing/rainEngine.test.ts
```

Expected output (4 lines confirming the leak still exists):
```
docs/src/components/landing/rainEngine.test.ts:14:test('ageToColor returns MATRIX_GREEN for age 3-5', () => {
docs/src/components/landing/rainEngine.ts:10:  if (age <= 5)   return 'rgb(0,255,65)';      // MATRIX_GREEN
docs/src/components/landing/rainEngine.ts:12:  if (age <= 16)  return 'rgb(0,140,30)';      // MATRIX_DIM
docs/src/components/landing/rainEngine.ts:13:  if (age <= 24)  return 'rgb(0,80,18)';       // MATRIX_DARK
```

- [ ] **Step 2: Verify AGENTS.md does not yet mention Conventional Commits (failing "test")**

```sh
grep -i 'conventional commits' AGENTS.md
```

Expected: no output (exits 1).

- [ ] **Step 3: Confirm clean working tree and create branch**

```sh
git status --short && git branch --show-current
```
Expected: empty status; branch `main`.

```sh
git checkout -b docs/conv-commits-and-finish-franchise-scrub
```

- [ ] **Step 4: Edit `rainEngine.ts` to swap MATRIX_* labels for PHOSPHOR_***

Edit the file, replacing exactly:
- Line 10: `      // MATRIX_GREEN` → `      // PHOSPHOR_BRIGHT`
- Line 12: `      // MATRIX_DIM`   → `      // PHOSPHOR_MID`
- Line 13: `      // MATRIX_DARK`  → `      // PHOSPHOR_DEEP`

The `rgb(...)` values stay unchanged.

- [ ] **Step 5: Edit `rainEngine.test.ts` test name**

Edit line 14, replacing exactly:
```
test('ageToColor returns MATRIX_GREEN for age 3-5', () => {
```
with:
```
test('ageToColor returns PHOSPHOR_BRIGHT for age 3-5', () => {
```

- [ ] **Step 6: Add `## Commit Messages` section to AGENTS.md**

Open `AGENTS.md`. After the existing `## Branching` section (which ends after line 16: "Merge back to `main` via pull request after review"), insert a blank line then paste the canonical block from the "Canonical AGENTS.md `## Commit Messages` section" preamble of this plan.

- [ ] **Step 7: Verify the franchise-leak "test" now passes**

```sh
grep -n 'MATRIX_' docs/src/components/landing/rainEngine.ts docs/src/components/landing/rainEngine.test.ts
```
Expected: no output (exits 1).

- [ ] **Step 8: Verify the Conv Commits "test" now passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a matching line.

- [ ] **Step 9: Build docs and run docs tests**

```sh
cd docs && bun run build && bun test && cd ..
```
Expected: build succeeds; all tests pass (the `ageToColor returns PHOSPHOR_BRIGHT for age 3-5` test included).

- [ ] **Step 10: Run `cargo` pre-commit checks per AGENTS.md**

```sh
cargo fmt -- --check && cargo clippy && cargo nextest run
```
Expected: zero warnings, zero failures.

- [ ] **Step 11: Stage, commit, push, open PR**

```sh
git add AGENTS.md docs/src/components/landing/rainEngine.ts docs/src/components/landing/rainEngine.test.ts
git commit -m "$(cat <<'EOF'
docs: adopt Conventional Commits + finish franchise-vocab scrub

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required commit-message format, with allowed types
table and breaking-change syntax.

Closes the franchise-vocabulary leak left by ff2a859: replaces
// MATRIX_GREEN/DIM/DARK comment labels in rainEngine.ts (lines 10/12/13)
with // PHOSPHOR_BRIGHT/MID/DEEP, matching the phosphor-themed naming the
prior commit established for the Rust-side colour constants. Updates the
matching test name in rainEngine.test.ts:14.

The rgb() values in rainEngine.ts are unchanged — only the label comments
and the test description string are updated. No runtime change.

Verification:
- bun run build: passes
- bun test: passes
- cargo fmt --check && cargo clippy && cargo nextest run: passes

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/conv-commits-and-finish-franchise-scrub
gh pr create --base main --head docs/conv-commits-and-finish-franchise-scrub --title "docs: adopt Conventional Commits + finish franchise-vocab scrub" --body "$(cat <<'EOF'
## Summary

- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0 with allowed-types table.
- Replace remaining franchise-vocab leaks in `rainEngine.ts` and `rainEngine.test.ts` (comment labels + one test name) — finishes the scrub commit ff2a859 started.

## Test plan

- [x] `bun run build` (in `docs/`) passes
- [x] `bun test` (in `docs/`) passes
- [x] `cargo fmt -- --check && cargo clippy && cargo nextest run` passes
- [x] `grep -i 'conventional commits' AGENTS.md` finds a match
- [x] `grep -rn 'MATRIX_' docs/src/components/landing/` returns no matches

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture the PR URL printed by `gh pr create`. Report it to the user.

---

### Task 2: `homebrew-tap` — Conv Commits doc + formula descs

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/homebrew-tap/AGENTS.md`
- Modify: `/Users/donbeave/Projects/jackin-project/homebrew-tap/Formula/jackin.rb:2`
- Modify: `/Users/donbeave/Projects/jackin-project/homebrew-tap/Formula/jackin-preview.rb:2`

**Branch:** `chore/conv-commits-and-formula-desc-scrub`

- [ ] **Step 1: Verify current franchise leak in formula descs (failing "test")**

```sh
cd /Users/donbeave/Projects/jackin-project/homebrew-tap
grep -n '^  desc' Formula/jackin.rb Formula/jackin-preview.rb
```
Expected: both lines contain `Matrix-inspired`.

- [ ] **Step 2: Confirm AGENTS.md state and clean tree**

```sh
git status --short
grep -i 'conventional commits' AGENTS.md
```
Expected: empty status; grep returns nothing (exits 1).

The current branch may not be `main` (audit found `docs/update-tag-ruleset-reality` here). If so, `git stash` any local-only state then `git checkout main` and `git pull --ff-only origin main` before continuing.

- [ ] **Step 3: Create branch from main**

```sh
git checkout main
git pull --ff-only origin main
git checkout -b chore/conv-commits-and-formula-desc-scrub
```

- [ ] **Step 4: Edit formula descs**

Edit `Formula/jackin.rb`. Replace line 2:
```
  desc "Matrix-inspired CLI for orchestrating AI coding agents at scale"
```
with:
```
  desc "CLI for orchestrating AI coding agents in Docker containers at scale"
```

Edit `Formula/jackin-preview.rb`. Apply the identical replacement on line 2.

- [ ] **Step 5: Add `## Commit Messages` section to AGENTS.md**

Read the current AGENTS.md. Append the canonical `## Commit Messages` section from this plan's preamble, after the last existing section. (`homebrew-tap` AGENTS.md has different structure — read it first to find the natural insertion point; if uncertain, append at end with one blank-line separator.)

- [ ] **Step 6: Verify scrub "test" passes**

```sh
grep -n 'Matrix' Formula/jackin.rb Formula/jackin-preview.rb
```
Expected: no output (exits 1).

- [ ] **Step 7: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 8: Brew formula sanity check (if `brew` is on the system)**

```sh
which brew && brew audit --strict --formula Formula/jackin.rb && brew audit --strict --formula Formula/jackin-preview.rb || echo "brew not available — visual diff only"
```
Expected: either `brew audit` passes, or "brew not available" message (the user will spot-check on their Mac if needed).

- [ ] **Step 9: Stage, commit, push, open PR**

```sh
git add AGENTS.md Formula/jackin.rb Formula/jackin-preview.rb
git commit -m "$(cat <<'EOF'
chore: scrub franchise prose from formula descs + adopt Conventional Commits

Replaces "Matrix-inspired CLI..." in both Formula/jackin.rb and
Formula/jackin-preview.rb with neutral product copy that aligns with the
upstream jackin Cargo.toml description (which was scrubbed in jackin
ff2a859).

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required format. The 91 release-bot commits in this
repo (jackin@preview <ver>+<sha>) are intentionally exempt — they're
machine-generated by upstream CI and embed a SHA cross-reference back to
the originating jackin build.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin chore/conv-commits-and-formula-desc-scrub
gh pr create --base main --head chore/conv-commits-and-formula-desc-scrub --title "chore: scrub franchise prose from formula descs + adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Replace `"Matrix-inspired CLI..."` in both formula `desc` lines with neutral copy.
- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0 (release-bot auto-commits exempt).

## Test plan

- [x] `grep 'Matrix' Formula/*.rb` returns no matches
- [x] `grep -i 'conventional commits' AGENTS.md` finds a match
- [ ] `brew audit --strict --formula Formula/jackin.rb` (run on Mac)
- [ ] `brew audit --strict --formula Formula/jackin-preview.rb` (run on Mac)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

### Task 3: `jackin-agent-smith` — Conv Commits doc + AGENTS.md/CLAUDE.md neutralization

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-agent-smith/AGENTS.md` (lines 3, 5)
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-agent-smith/CLAUDE.md` (line 5)

**Branch:** `docs/neutralize-personality-framing`

- [ ] **Step 1: Read current AGENTS.md and CLAUDE.md to confirm exact prose to replace**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-agent-smith
cat AGENTS.md
cat CLAUDE.md
```

Look for the literal phrase `Agent Smith` on the AGENTS.md lines noted (3, 5) and CLAUDE.md line 5. Confirm the prose matches the audit description: "An 'Agent Smith' personality with the `code-review` and `feature-dev` plugins pre-configured" or similar.

- [ ] **Step 2: Confirm clean tree, create branch from main**

```sh
git status --short
git checkout main
git pull --ff-only origin main
git checkout -b docs/neutralize-personality-framing
```

- [ ] **Step 3: Replace AGENTS.md "Agent Smith personality" prose**

Edit AGENTS.md. Replace any sentence like:
> An 'Agent Smith' personality with the `code-review` and `feature-dev` plugins pre-configured...

with the neutral form:
> A Claude Code agent image extending `projectjackin/construct:trixie` with the `code-review` and `feature-dev` plugins pre-configured for code-review-focused work.

Repeat for both occurrences (lines 3 and 5 per audit).

The repo NAME `jackin-agent-smith` and any URL containing it stay unchanged — this edit only affects descriptive prose.

- [ ] **Step 4: Apply the same neutralization to CLAUDE.md line 5**

Replace any sentence describing the agent as having an "Agent Smith" personality with the same neutral form used in AGENTS.md.

- [ ] **Step 5: Add `## Commit Messages` section to AGENTS.md**

Append the canonical section from this plan's preamble after the existing structure, as a new top-level section.

- [ ] **Step 6: Verify the franchise-prose "test" passes**

```sh
grep -ni "agent smith" AGENTS.md CLAUDE.md
```
Expected: no output (exits 1). The repo's own URL/identifier doesn't appear in these files; if it does as part of, e.g., a `git clone` example, that's fine — only flag prose.

If the grep returns output, inspect each match: keep clones/URLs/literal-identifier mentions, scrub anything in prose.

- [ ] **Step 7: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 8: Stage, commit, push, open PR**

```sh
git add AGENTS.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: neutralize agent personality framing + adopt Conventional Commits

Rewrites the AGENTS.md and CLAUDE.md prose that described this image as
having an 'Agent Smith' personality. Replaces with neutral, functional
language: "A Claude Code agent image extending projectjackin/construct:trixie
with the code-review and feature-dev plugins pre-configured for
code-review-focused work."

The repo identifier (jackin-agent-smith) is intentionally kept — it's
pinned to public URLs and renaming is a breaking change deferred to a
future deprecation-window PR.

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required format.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/neutralize-personality-framing
gh pr create --base main --head docs/neutralize-personality-framing --title "docs: neutralize agent personality framing + adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Remove "Agent Smith personality" framing from AGENTS.md and CLAUDE.md; replace with neutral functional description.
- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0.

Repo identifier `jackin-agent-smith` is intentionally kept (pinned to public URLs).

## Test plan

- [x] `grep -i "agent smith" AGENTS.md CLAUDE.md` returns no prose matches
- [x] `grep -i 'conventional commits' AGENTS.md` finds a match

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

### Task 4: `jackin-the-architect` — Conv Commits doc + README quote removal + AGENTS.md prose

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-the-architect/README.md:5`
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-the-architect/AGENTS.md:3`

**Branch:** `docs/remove-matrix-quote-and-soften-rationale`

- [ ] **Step 1: Verify current franchise leaks (failing "test")**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-the-architect
grep -n 'I created the Matrix\|I am the Architect' README.md
```
Expected: line 5 of README.md matches `> "I am the Architect. I created the Matrix."`.

```sh
sed -n '1,10p' AGENTS.md
```
Examine line 3 to see the current "Architect" rationale prose.

- [ ] **Step 2: Confirm clean tree, create branch from main**

```sh
git status --short
git checkout main
git pull --ff-only origin main
git checkout -b docs/remove-matrix-quote-and-soften-rationale
```

- [ ] **Step 3: Remove the Matrix quote from README.md line 5**

Delete the entire blockquote line (`> "I am the Architect. I created the Matrix."`). If line 4 or 6 is a blank line that exists only to separate this quote from surrounding content, also delete one of them so the layout doesn't have a double-blank gap.

- [ ] **Step 4: Soften AGENTS.md line 3 rationale**

Replace whatever the current line 3 reads (something like `Named "The Architect" because...`) with:

> Named `the-architect` because it has the broadest operator capability of the agent images — it can manage the entire `jackin-project` repo collection.

The class identifier `the-architect` (lowercase, hyphenated, in backticks as code) is kept; only the franchise-quoted "The Architect" prose is softened.

- [ ] **Step 5: Add `## Commit Messages` section to AGENTS.md**

Append the canonical section from the plan preamble.

- [ ] **Step 6: Verify scrub "test" passes**

```sh
grep -ni 'created the matrix\|"the architect"' README.md AGENTS.md
```
Expected: no output (exits 1).

- [ ] **Step 7: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 8: Stage, commit, push, open PR**

```sh
git add README.md AGENTS.md
git commit -m "$(cat <<'EOF'
docs: remove Matrix quote from README + adopt Conventional Commits

Removes the blockquote "I am the Architect. I created the Matrix." from
README.md line 5 — direct franchise quote in marketing copy.

Softens AGENTS.md rationale prose: "Named the-architect because it has
the broadest operator capability of the agent images — it can manage the
entire jackin-project repo collection." The class identifier
`the-architect` is kept in backticks as code; only the franchise-styled
"The Architect" prose is replaced.

The repo identifier (jackin-the-architect) is intentionally kept — it's
pinned to public URLs and renaming is a breaking change deferred to a
future deprecation-window PR.

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/remove-matrix-quote-and-soften-rationale
gh pr create --base main --head docs/remove-matrix-quote-and-soften-rationale --title "docs: remove Matrix quote from README + adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Remove `> "I am the Architect. I created the Matrix."` quote from README.md.
- Soften AGENTS.md rationale prose for the agent class identifier.
- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0.

Repo identifier `jackin-the-architect` and class identifier `the-architect` are intentionally kept (pinned to public URLs / config).

## Test plan

- [x] `grep -i 'created the matrix' README.md` returns no matches
- [x] `grep -i 'conventional commits' AGENTS.md` finds a match

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

### Task 5: `jackin-dev` — Conv Commits doc only

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-dev/AGENTS.md`

**Branch:** `docs/adopt-conventional-commits`

- [ ] **Step 1: Confirm clean tree, create branch from main**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-dev
git status --short
git checkout main
git pull --ff-only origin main
git checkout -b docs/adopt-conventional-commits
```

- [ ] **Step 2: Verify AGENTS.md state (failing "test")**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: no output (exits 1).

- [ ] **Step 3: Append `## Commit Messages` section**

Append the canonical block from this plan's preamble at the end of AGENTS.md, after one blank-line separator.

- [ ] **Step 4: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 5: Stage, commit, push, open PR**

```sh
git add AGENTS.md
git commit -m "$(cat <<'EOF'
docs: adopt Conventional Commits

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required commit-message format for this repo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/adopt-conventional-commits
gh pr create --base main --head docs/adopt-conventional-commits --title "docs: adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0.

## Test plan

- [x] `grep -i 'conventional commits' AGENTS.md` finds a match

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

### Task 6: `jackin-marketplace` — Conv Commits doc only

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-marketplace/AGENTS.md`

**Branch:** `docs/adopt-conventional-commits`

- [ ] **Step 1: Confirm clean tree, create branch from main**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-marketplace
git status --short
git checkout main
git pull --ff-only origin main
git checkout -b docs/adopt-conventional-commits
```

- [ ] **Step 2: Verify AGENTS.md state (failing "test")**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: no output (exits 1).

- [ ] **Step 3: Append `## Commit Messages` section**

Append the canonical block at the end of AGENTS.md.

- [ ] **Step 4: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 5: Stage, commit, push, open PR**

```sh
git add AGENTS.md
git commit -m "$(cat <<'EOF'
docs: adopt Conventional Commits

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required commit-message format for this repo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/adopt-conventional-commits
gh pr create --base main --head docs/adopt-conventional-commits --title "docs: adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0.

## Test plan

- [x] `grep -i 'conventional commits' AGENTS.md` finds a match

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

### Task 7: `jackin-github-terraform` — Conv Commits doc only

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/jackin-github-terraform/AGENTS.md`

**Branch:** `docs/adopt-conventional-commits`

- [ ] **Step 1: Confirm clean tree, switch to main, pull**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-github-terraform
git status --short
```

If currently on `chore/refresh-lock-file-for-opentofu` (sibling agent's branch), do NOT delete or alter it. Just `git checkout main` after confirming working tree is clean.

```sh
git checkout main
git pull --ff-only origin main
git checkout -b docs/adopt-conventional-commits
```

- [ ] **Step 2: Verify AGENTS.md state (failing "test")**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: no output (exits 1).

- [ ] **Step 3: Append `## Commit Messages` section**

Append the canonical block at the end of AGENTS.md.

- [ ] **Step 4: Run `tofu fmt -check && tofu validate` for the existing terraform**

```sh
tofu fmt -check && tofu init -backend=false && tofu validate
```
Expected: pass (no terraform files were touched, but verify environment is sane before committing).

- [ ] **Step 5: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 6: Stage, commit, push, open PR**

```sh
git add AGENTS.md
git commit -m "$(cat <<'EOF'
docs: adopt Conventional Commits

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required commit-message format for this repo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/adopt-conventional-commits
gh pr create --base main --head docs/adopt-conventional-commits --title "docs: adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0.

## Test plan

- [x] `grep -i 'conventional commits' AGENTS.md` finds a match
- [x] `tofu fmt -check && tofu validate` passes (no terraform changed)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

### Task 8: `validate-agent-action` — Conv Commits doc only

**Files:**
- Modify: `/Users/donbeave/Projects/jackin-project/validate-agent-action/AGENTS.md`

**Branch:** `docs/adopt-conventional-commits`

- [ ] **Step 1: Confirm clean tree, create branch from main**

```sh
cd /Users/donbeave/Projects/jackin-project/validate-agent-action
git status --short
git checkout main
git pull --ff-only origin main
git checkout -b docs/adopt-conventional-commits
```

- [ ] **Step 2: Verify AGENTS.md state (failing "test")**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: no output (exits 1).

- [ ] **Step 3: Append `## Commit Messages` section**

Append the canonical block at the end of AGENTS.md.

- [ ] **Step 4: Verify Conv Commits doc "test" passes**

```sh
grep -i 'conventional commits' AGENTS.md
```
Expected: a match.

- [ ] **Step 5: Stage, commit, push, open PR**

```sh
git add AGENTS.md
git commit -m "$(cat <<'EOF'
docs: adopt Conventional Commits

Adds a ## Commit Messages section to AGENTS.md documenting Conventional
Commits 1.0.0 as the required commit-message format for this repo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git push -u origin docs/adopt-conventional-commits
gh pr create --base main --head docs/adopt-conventional-commits --title "docs: adopt Conventional Commits" --body "$(cat <<'EOF'
## Summary

- Add `## Commit Messages` section to `AGENTS.md` documenting Conventional Commits 1.0.0.

## Test plan

- [x] `grep -i 'conventional commits' AGENTS.md` finds a match

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture and report the PR URL.

---

# 🛑 Pause Point — Phase 1 → Phase 2 Gate

**Do not proceed to Task 9 until ALL of the following are true:**

1. All 8 Phase 1 PRs from Tasks 1–8 are MERGED to `main` in their respective repos.
2. Each repo's local `main` is updated: `git checkout main && git pull --ff-only origin main` per repo.
3. The user has explicitly confirmed: "sibling agents are paused; proceed to Phase 2."
4. The user has confirmed they accept that all `git log` SHAs after the rewrite point in each repo will change, breaking any external commit URLs.

Report progress to the user and wait for explicit go-ahead before continuing.

---

# Phase 2 — Per-repo history rewrite (Tasks 9–19)

---

### Task 9: Install `git-filter-repo`

**Files:** none (system dependency).

- [ ] **Step 1: Verify it is currently NOT installed (failing "test")**

```sh
git filter-repo --help 2>&1 | head -1
```
Expected: `git: 'filter-repo' is not a git command.`

- [ ] **Step 2: Install via `pip3 --user`**

```sh
pip3 install --user git-filter-repo
```

If `pip3` is not available, install via system package manager (e.g., `apt-get install git-filter-repo` on Debian/Ubuntu, `brew install git-filter-repo` on macOS — but the agent's environment is Linux per session bootstrap, so `pip3 --user` is the canonical path).

- [ ] **Step 3: Verify install (passing "test")**

```sh
git filter-repo --version
```
Expected: a version number, e.g., `2.45.0` or higher.

If `git filter-repo` still doesn't resolve, ensure `~/.local/bin` is on `$PATH` and re-source the shell.

---

### Task 10: `jackin` — Build mapping table + dry run on fresh clone

**Files:**
- Create: `/tmp/jackin-rewrite-2026-04-21-mapping.py` (filter-repo callback script)
- Working directory: fresh clone at `/tmp/jackin-rewrite-2026-04-21-clone/`

- [ ] **Step 1: Pull latest `main` in working repo and capture pre-state**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin
git checkout main && git pull --ff-only origin main
git log --pretty=format:'%H %s' main > /tmp/jackin-pre-rewrite-log.txt
wc -l /tmp/jackin-pre-rewrite-log.txt
```
Expected: line count matches current commit count (~431 after Phase 1 PR merges).

- [ ] **Step 2: Identify all non-conforming commit subjects**

```sh
git log main --pretty=format:'%H %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: ' > /tmp/jackin-non-conforming.txt
wc -l /tmp/jackin-non-conforming.txt
```
Expected: ~71 lines (per audit). Inspect the file:

```sh
cat /tmp/jackin-non-conforming.txt
```

- [ ] **Step 3: Write the filter-repo callback Python script**

Create `/tmp/jackin-rewrite-2026-04-21-mapping.py` with the following content:

```python
#!/usr/bin/env python3
"""
git filter-repo --commit-callback for jackin Conv-Commits backfill.

For each commit, rewrite the subject line if it doesn't already conform
to Conventional Commits.
"""
import re

CONV_RE = re.compile(rb'^(feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: ')

# Hand-curated mapping for free-form subjects. Generated from inspection
# of /tmp/jackin-non-conforming.txt. Keys are exact byte-string subjects;
# values are the replacement subjects. Anything not matching a rule below
# falls through to the auto-rules at the bottom.
EXPLICIT_MAP = {
    b"Drop franchise-specific vocabulary across the repo": b"docs: drop franchise-specific vocabulary across the repo",
    b"Landing page + Tempo-style docs theme (#93)": b"feat(docs): landing page + Tempo-style docs theme (#93)",
    b"Add AGENTS.md and CLAUDE.md to todo/, link todo from roadmap": b"docs: add AGENTS.md and CLAUDE.md to todo/, link todo from roadmap",
    # ... USER-REVIEWED mapping for the remaining 30 free-form subjects
    # is hand-built in Step 4 below before this script is finalized.
}

def commit_callback(commit, metadata):
    subject = commit.message.split(b'\n', 1)[0]

    if CONV_RE.match(subject):
        return  # already conforms

    # Rule 1: GitHub merge commits
    m = re.match(rb'^Merge pull request #(\d+) from [^/]+/(.+)$', subject)
    if m:
        pr_num, branch = m.group(1), m.group(2)
        new_subject = b"chore: merge pull request #" + pr_num + b" (" + branch + b")"
        commit.message = new_subject + commit.message[len(subject):]
        return

    # Rule 2: release: vX.Y.Z
    m = re.match(rb'^release: (v\d+\.\d+\.\d+.*)$', subject)
    if m:
        ver = m.group(1)
        new_subject = b"chore(release): " + ver
        commit.message = new_subject + commit.message[len(subject):]
        return

    # Rule 3: explicit hand-curated map
    if subject in EXPLICIT_MAP:
        new_subject = EXPLICIT_MAP[subject]
        commit.message = new_subject + commit.message[len(subject):]
        return

    # Fallthrough: leave alone, will be flagged in dry-run diff
    pass
```

- [ ] **Step 4: Hand-curate the remaining EXPLICIT_MAP entries**

Open `/tmp/jackin-non-conforming.txt`. For every line that is NOT a `Merge pull request` and NOT a `release: vX.Y.Z`, propose a Conv-Commits subject and add it to `EXPLICIT_MAP`. Use these heuristics:
- If the subject begins with a verb describing a change ("Add X", "Update Y", "Improve Z"), prefix `chore:` or `feat:` based on whether the change adds new user-visible behavior.
- If it describes a docs change, use `docs:`.
- If it's a security/CI change, use `ci:` or `chore(ci):`.

After the mapping is complete, **save the proposed mapping to `/tmp/jackin-mapping-proposal.txt`** in the format:

```
OLD: <original subject>
NEW: <proposed subject>
```

**Show this file to the user and wait for explicit approval** before continuing to Step 5. The user may amend mappings.

- [ ] **Step 5: Make a fresh clone for the dry run**

```sh
rm -rf /tmp/jackin-rewrite-2026-04-21-clone
git clone https://github.com/jackin-project/jackin /tmp/jackin-rewrite-2026-04-21-clone
cd /tmp/jackin-rewrite-2026-04-21-clone
```

- [ ] **Step 6: Run filter-repo with the callback (dry run by virtue of working in a throwaway clone)**

```sh
cd /tmp/jackin-rewrite-2026-04-21-clone
git filter-repo --commit-callback "$(cat /tmp/jackin-rewrite-2026-04-21-mapping.py | sed -n '/^def commit_callback/,$p' | sed '1d')" --force
```

If the inline `--commit-callback` form is awkward, use the file-based form:
```sh
git filter-repo --refs main --commit-callback "$(cat /tmp/jackin-rewrite-2026-04-21-mapping.py)" --force
```

Refer to `git filter-repo --help` for the exact syntax of passing a callback file. The script structure above defines `commit_callback(commit, metadata)` which is the expected signature.

- [ ] **Step 7: Verify the rewrite produced 100% conforming commits**

```sh
cd /tmp/jackin-rewrite-2026-04-21-clone
TOTAL=$(git log main --pretty=format:'%H' | wc -l)
CONF=$(git log main --pretty=format:'%H %s' | grep -cE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: ')
echo "Total: $TOTAL  Conforming: $CONF  Non-conforming: $((TOTAL - CONF))"
```
Expected: `Non-conforming: 0` (or a small documented exception count if some entries were intentionally left).

- [ ] **Step 8: Save before/after subject diff for user review**

```sh
cd /tmp/jackin-rewrite-2026-04-21-clone
git log main --pretty=format:'%h %s' > /tmp/jackin-post-rewrite-log.txt
diff /tmp/jackin-pre-rewrite-log.txt /tmp/jackin-post-rewrite-log.txt > /tmp/jackin-rewrite-diff.txt
wc -l /tmp/jackin-rewrite-diff.txt
head -200 /tmp/jackin-rewrite-diff.txt
```

Report the diff file path to the user.

---

### Task 11: `jackin` — User review of rewritten history

- [ ] **Step 1: Present the rewrite summary to the user**

In a message to the user, include:
- Path: `/tmp/jackin-rewrite-diff.txt` (full before/after subject diff)
- Path: `/tmp/jackin-mapping-proposal.txt` (the EXPLICIT_MAP rationale)
- Counts: total commits rewritten, total conforming after, total non-conforming after
- A sample of 20 representative rewrites: `head -40 /tmp/jackin-rewrite-diff.txt`

Ask: "Approve this rewritten history? Any subjects you want me to amend before the force-push? (Reply: 'approve', 'amend X to Y', or 'abort')"

- [ ] **Step 2: Wait for explicit user approval**

Do not proceed to Task 12 until the user replies "approve" (or equivalent unambiguous yes). If they request amendments, return to Task 10 Step 4, update `EXPLICIT_MAP`, re-run Step 6 onward.

---

### Task 12: `jackin` — Tag pre-rewrite SHA + force-push rewritten main

**This is the destructive step.** Final user approval already captured in Task 11.

- [ ] **Step 1: Tag the pre-rewrite tip in the working repo and push the tag**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin
git checkout main && git pull --ff-only origin main
PRE_REWRITE_SHA=$(git rev-parse HEAD)
echo "Pre-rewrite SHA: $PRE_REWRITE_SHA"
git tag -a pre-rewrite-2026-04-21 $PRE_REWRITE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21. All SHAs after this point are stable via this tag even though main has been rewritten."
git push origin pre-rewrite-2026-04-21
```
Expected: tag pushed successfully.

- [ ] **Step 2: Push the rewritten history from the throwaway clone**

```sh
cd /tmp/jackin-rewrite-2026-04-21-clone
git remote -v
# Verify remote points to github.com/jackin-project/jackin
git push --force-with-lease=main:$PRE_REWRITE_SHA origin main
```

The `--force-with-lease=main:$PRE_REWRITE_SHA` form aborts the push if `origin/main` has moved since the tag was created — this is the safe form of force-push for this scenario.

If the push aborts due to drift, return to Task 10 Step 1 and rebuild from the new tip.

- [ ] **Step 3: Verify remote main now matches the rewrite**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin
git fetch origin
git log origin/main --pretty=format:'%h %s' | head -10
```
Expected: tip subjects match the post-rewrite log.

- [ ] **Step 4: Reset local main to the rewritten remote**

```sh
git checkout main
git reset --hard origin/main
```

- [ ] **Step 5: Verify the conformance "test" passes on the live repo**

```sh
TOTAL=$(git log main --pretty=format:'%H' | wc -l)
CONF=$(git log main --pretty=format:'%H %s' | grep -cE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: ')
echo "Live: Total $TOTAL Conforming $CONF Non-conforming $((TOTAL - CONF))"
```
Expected: `Non-conforming: 0` (or documented exception count).

- [ ] **Step 6: Spot-check on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Visual confirmation: the tip commit subject reads as expected on github.com.

- [ ] **Step 7: Report completion to user**

Include:
- Pre-rewrite SHA (so old links can be salvaged via `https://github.com/jackin-project/jackin/commit/$PRE_REWRITE_SHA`)
- New tip SHA
- Tag name (`pre-rewrite-2026-04-21`)
- Conformance after: `100%` or the documented exception count

---

### Task 13: `jackin-agent-smith` — interactive rebase rewrite

**Repo has 3 non-conforming commits per audit. Use interactive rebase since `filter-repo` is overkill.**

- [ ] **Step 1: Pull latest, capture pre-state, identify rewrite range**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-agent-smith
git checkout main && git pull --ff-only origin main
git log --pretty=format:'%H %s' > /tmp/jackin-agent-smith-pre.txt
git log --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: 3 commits listed. Note the SHA of the OLDEST listed commit; the rebase will start one commit before that.

Per audit, the 3 are:
- `9eca702` — `Merge pull request #1 from jackin-project/rename/construct-image`
- `1316b73` — `Rename construct image to projectjackin/construct`
- `8f1fafd` — `Rename to agent-smith and add identity config`

(Plus possibly `5fe58e7` `Update construct image reference to donbeave/jackin-construct:trixie` and `85ea1ce` `ci: pin validation workflow actions` — re-confirm the actual list since the audit was a snapshot.)

- [ ] **Step 2: Tag the pre-rewrite tip and push it**

```sh
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 3: Determine the parent of the oldest non-conforming commit**

```sh
OLDEST=<the oldest non-conforming SHA from Step 1>
PARENT=$(git rev-parse $OLDEST^)
echo "Rebase from: $PARENT"
```

- [ ] **Step 4: Run interactive rebase**

```sh
git rebase -i $PARENT
```

In the editor, change `pick` to `reword` for each non-conforming commit. Then save. For each `reword`, git will open the commit message — replace the subject line per these rules (apply same callbacks as Task 10 mapping):
- `Merge pull request #N from <owner>/<branch>` → `chore: merge pull request #N (<branch>)`
- `Rename construct image to projectjackin/construct` → `refactor: rename construct image to projectjackin/construct`
- `Rename to agent-smith and add identity config` → `refactor: rename to agent-smith and add identity config`
- `Update construct image reference to donbeave/jackin-construct:trixie` → `chore: update construct image reference to donbeave/jackin-construct:trixie`
- `ci: pin validation workflow actions` → already conforms (`ci:` prefix), leave alone

If during `reword`, the body contains franchise prose mentioning "Agent Smith personality" or similar, also scrub the body.

- [ ] **Step 5: Verify conformance "test" passes locally**

```sh
git log main --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: empty.

- [ ] **Step 6: Show user the proposed force-push**

```sh
git log main --pretty=format:'%h %s' | head -10
```

Ask user: "Force-push `jackin-agent-smith` main? Pre-rewrite tag already pushed."

- [ ] **Step 7: Force-push with lease**

```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 8: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip SHA + pre-rewrite tag to user.

---

### Task 14: `jackin-the-architect` — interactive rebase rewrite

**Files:** none (history-only edit).

Repo has 2 non-conforming commits per audit:
- `24f5bea` — `Merge pull request #1 from jackin-project/rename/construct-image`
- `583069b` — `Rename construct image to projectjackin/construct`

Plus possibly franchise prose in commit bodies (`ae271b0` body mentions "Matrix green").

- [ ] **Step 1: Pull latest, identify non-conforming, tag, push tag**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-the-architect
git checkout main && git pull --ff-only origin main
git log --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 2: Inspect commit bodies that may contain franchise prose**

```sh
for sha in ae271b0 eedb2a5 35dc6dc; do
  echo "=== $sha ==="
  git show -s --format='%H%n%s%n%b' $sha
done
```

Note any franchise-themed prose in the body of these commits — they will be scrubbed during reword.

- [ ] **Step 3: Determine rebase point and run interactive rebase**

```sh
OLDEST=<the oldest non-conforming SHA, likely 583069b>
PARENT=$(git rev-parse $OLDEST^)
git rebase -i $PARENT
```

In the editor:
- Mark each non-conforming commit as `reword`.
- Mark `ae271b0`, `eedb2a5`, `35dc6dc` as `reword` even if their subjects conform — to scrub franchise prose from bodies.

Reword rules (same callbacks as before):
- `Merge pull request #1 from jackin-project/rename/construct-image` → `chore: merge pull request #1 (rename/construct-image)`
- `Rename construct image to projectjackin/construct` → `refactor: rename construct image to projectjackin/construct`

For commit bodies, replace any "Matrix green" / "I am the Architect" prose with neutral equivalents (e.g., "phosphor green").

- [ ] **Step 4: Verify conformance "test" passes locally**

```sh
git log main --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: empty.

- [ ] **Step 5: Verify body-prose scrub**

```sh
git log -i --grep='matrix\|i am the architect' --format='%h %s'
```
Expected: no matches (or only matches on commits that were intentionally kept).

- [ ] **Step 6: Show user the proposed force-push**

```sh
git log main --pretty=format:'%h %s' | head -10
```

Ask user for go-ahead.

- [ ] **Step 7: Force-push with lease**

```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 8: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip SHA + pre-rewrite tag.

---

### Task 15: `jackin-dev` — interactive rebase rewrite

Repo has 1 merge commit + 1 initial commit per audit.

- [ ] **Step 1: Pull, tag, push tag**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-dev
git checkout main && git pull --ff-only origin main
git log --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 2: Run interactive rebase from root**

Since this includes the initial commit, use `--root`:

```sh
git rebase -i --root
```

In the editor, mark `pick` as `reword` for:
- `Initial commit` → `chore: initial commit`
- `Merge pull request #1 from jackin-project/fix/marketplace-org-reference` → `chore: merge pull request #1 (fix/marketplace-org-reference)`

- [ ] **Step 3: Verify conformance**

```sh
git log main --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: empty.

- [ ] **Step 4: Show user, confirm, force-push with lease**

```sh
git log main --pretty=format:'%h %s'
```

Ask user. On approval:
```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 5: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip + tag.

---

### Task 16: `jackin-marketplace` — interactive rebase rewrite

Repo has 1 merge commit + 1 initial commit per audit.

- [ ] **Step 1: Pull, tag, push tag**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-marketplace
git checkout main && git pull --ff-only origin main
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 2: Interactive rebase from root**

```sh
git rebase -i --root
```

Reword:
- `Initial commit` → `chore: initial commit`
- `Merge pull request #1 from jackin-project/fix/project-org-references` → `chore: merge pull request #1 (fix/project-org-references)`

- [ ] **Step 3: Verify conformance**

```sh
git log main --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: empty.

- [ ] **Step 4: Show user, confirm, force-push**

```sh
git log main --pretty=format:'%h %s'
```

On approval:
```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 5: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip + tag.

---

### Task 17: `jackin-github-terraform` — interactive rebase rewrite

Repo has 1 initial + 1 merge commit per audit.

**SAFETY NOTE**: Sibling agent may still be on `chore/refresh-lock-file-for-opentofu` per audit. The Phase 1 → Phase 2 gate already required user confirmation that agents are paused — but double-check before force-push.

- [ ] **Step 1: Confirm sibling-agent branch state**

```sh
cd /Users/donbeave/Projects/jackin-project/jackin-github-terraform
git branch -a
```

If `chore/refresh-lock-file-for-opentofu` still exists locally OR on remote: confirm with the user that it's safe to force-push main. The sibling branch will need to be rebased against the rewritten main; that's the sibling agent's problem to solve, not this task's.

- [ ] **Step 2: Pull, tag, push tag**

```sh
git checkout main && git pull --ff-only origin main
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 3: Interactive rebase from root**

```sh
git rebase -i --root
```

Reword:
- `Initial Terraform setup for GitHub branch protection` → `chore: initial Terraform setup for GitHub branch protection`
- `Merge pull request #1 from jackin-project/feat/repo-merge-policy` → `chore: merge pull request #1 (feat/repo-merge-policy)`

- [ ] **Step 4: Verify conformance**

```sh
git log main --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: empty.

- [ ] **Step 5: Re-run terraform validation as sanity check**

```sh
tofu fmt -check && tofu init -backend=false && tofu validate
```
Expected: pass (no terraform code changed; rewrite was history-only).

- [ ] **Step 6: Show user, confirm, force-push**

```sh
git log main --pretty=format:'%h %s'
```

On approval:
```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 7: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip + tag.

---

### Task 18: `validate-agent-action` — interactive rebase rewrite

Repo has 1 initial + 1 pre-Conv commit per audit:
- `f4be831` — `Initial commit`
- `8d103fd` — `Add composite GitHub Action for validating jackin agent repos`

- [ ] **Step 1: Pull, tag, push tag**

```sh
cd /Users/donbeave/Projects/jackin-project/validate-agent-action
git checkout main && git pull --ff-only origin main
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before Conventional Commits backfill on 2026-04-21."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 2: Interactive rebase from root**

```sh
git rebase -i --root
```

Reword:
- `Initial commit` → `chore: initial commit`
- `Add composite GitHub Action for validating jackin agent repos` → `feat: add composite GitHub Action for validating jackin agent repos`

- [ ] **Step 3: Verify conformance**

```sh
git log main --pretty=format:'%h %s' | grep -vE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: '
```
Expected: empty.

- [ ] **Step 4: Show user, confirm, force-push**

```sh
git log main --pretty=format:'%h %s'
```

On approval:
```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 5: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip + tag.

---

### Task 19: `homebrew-tap` — selective rebase (4 manual commits only, skip 91 release-bot commits)

**Per spec Decision 1: do NOT rewrite the 91 release-bot commits matching `jackin@preview <ver>+<sha>`.** Only the 4 manual commits get reworded.

The 4 manual commits per audit are:
- `Update jackin to 0.2.0`
- `jackin 0.3.0`
- `jackin 0.4.0`
- `jackin 0.5.0`

- [ ] **Step 1: Pull, identify exact SHAs of the 4 manual commits**

```sh
cd /Users/donbeave/Projects/jackin-project/homebrew-tap
git checkout main && git pull --ff-only origin main
git log --pretty=format:'%H %s' main | grep -E '^[a-f0-9]+ (Update jackin to|jackin) [0-9]' > /tmp/homebrew-manual-commits.txt
cat /tmp/homebrew-manual-commits.txt
```
Expected: 4 lines.

- [ ] **Step 2: Tag pre-rewrite tip and push tag**

```sh
PRE_SHA=$(git rev-parse HEAD)
git tag -a pre-rewrite-2026-04-21 $PRE_SHA -m "Snapshot of main before selective Conventional Commits backfill on 2026-04-21. Only the 4 manual jackin version-bump commits were reworded; 91 release-bot auto-commits intentionally preserved."
git push origin pre-rewrite-2026-04-21
```

- [ ] **Step 3: Run interactive rebase covering the oldest manual commit's parent through HEAD**

```sh
OLDEST=$(tail -1 /tmp/homebrew-manual-commits.txt | awk '{print $1}')
PARENT=$(git rev-parse $OLDEST^)
git rebase -i $PARENT
```

In the editor, find the 4 lines matching the manual commits. Mark each as `reword`. Leave all other lines (the release-bot commits) as `pick`.

Reword to:
- `Update jackin to 0.2.0` → `chore(release): jackin 0.2.0`
- `jackin 0.3.0` → `chore(release): jackin 0.3.0`
- `jackin 0.4.0` → `chore(release): jackin 0.4.0`
- `jackin 0.5.0` → `chore(release): jackin 0.5.0`

- [ ] **Step 4: Verify the bot commits were preserved (NOT rewritten)**

```sh
git log main --pretty=format:'%h %s' | grep -cE '^[a-f0-9]+ jackin@preview '
```
Expected: a count matching the prior bot-commit count (~91), unchanged.

- [ ] **Step 5: Verify the 4 manual commits now conform**

```sh
git log main --pretty=format:'%h %s' | grep -E '^[a-f0-9]+ (jackin |Update jackin )'
```
Expected: empty (the old subjects no longer match — they're now `chore(release): ...`).

```sh
git log main --pretty=format:'%h %s' | grep -cE '^[a-f0-9]+ chore\(release\): jackin [0-9]'
```
Expected: 4.

- [ ] **Step 6: Show user, confirm, force-push**

```sh
git log main --pretty=format:'%h %s' | head -20
```

On approval:
```sh
git push --force-with-lease=main:$PRE_SHA origin main
```

- [ ] **Step 7: Verify on GitHub**

```sh
gh browse $(git rev-parse HEAD)
```

Report new tip + tag.

---

# Final Verification (all phases complete)

- [ ] **Repo-by-repo conformance check**

For each of the eight repos, run from the repo root:

```sh
TOTAL=$(git log main --pretty=format:'%H' | wc -l)
CONF=$(git log main --pretty=format:'%H %s' | grep -cE ' (feat|fix|docs|chore|refactor|test|build|ci|perf|style|revert)(\([^)]+\))?!?: ')
echo "$(basename $(pwd)): Total $TOTAL Conforming $CONF Non-conf $((TOTAL - CONF))"
```

Expected for each repo (after all phases):

| Repo | Expected Non-conf |
|---|---|
| `jackin` | 0 |
| `homebrew-tap` | 91 (release-bot commits, intentionally preserved) |
| `jackin-agent-smith` | 0 |
| `jackin-dev` | 0 |
| `jackin-github-terraform` | 0 |
| `jackin-marketplace` | 0 |
| `jackin-the-architect` | 0 |
| `validate-agent-action` | 0 |

- [ ] **Pre-rewrite tag exists in each rewritten repo**

```sh
for repo in jackin homebrew-tap jackin-agent-smith jackin-dev jackin-github-terraform jackin-marketplace jackin-the-architect validate-agent-action; do
  cd /Users/donbeave/Projects/jackin-project/$repo
  echo -n "$repo: "
  git tag --list 'pre-rewrite-2026-04-21'
done
```
Expected: every repo prints `pre-rewrite-2026-04-21`.

- [ ] **Conv-Commits doc exists in each repo's AGENTS.md**

```sh
for repo in jackin homebrew-tap jackin-agent-smith jackin-dev jackin-github-terraform jackin-marketplace jackin-the-architect validate-agent-action; do
  cd /Users/donbeave/Projects/jackin-project/$repo
  echo -n "$repo: "
  grep -c -i 'conventional commits' AGENTS.md
done
```
Expected: every repo prints a count ≥ 1.

- [ ] **Working trees free of franchise prose**

```sh
for repo in jackin homebrew-tap jackin-agent-smith jackin-the-architect; do
  cd /Users/donbeave/Projects/jackin-project/$repo
  echo "=== $repo ==="
  grep -ri --include='*.md' --include='*.toml' --include='*.rb' --include='*.ts' --include='*.tsx' --include='*.rs' \
    -E 'matrix-inspired|i am the architect.*matrix|agent smith personality|MATRIX_GREEN|MATRIX_DIM|MATRIX_DARK' \
    . 2>/dev/null | grep -v -E 'docs/dist/|target/|node_modules/|tmp/' || echo "  (clean)"
done
```
Expected: every repo prints `(clean)`.

- [ ] **Final report to user**

Summarize:
- 8 PRs merged in Phase 1 (with URLs)
- 8 repos rewritten in Phase 2 (with new tip SHAs and pre-rewrite tag references)
- Pre-rewrite tag URLs for each repo (so old SHAs remain reachable)
- Any documented exceptions (homebrew-tap's 91 bot commits)
