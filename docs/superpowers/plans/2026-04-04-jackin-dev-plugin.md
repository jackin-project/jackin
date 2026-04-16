# jackin-dev Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a Claude Code plugin (`jackin-project/jackin-dev`) with three release skills: `/release-check`, `/release-notes`, and `/release`.

**Architecture:** A standalone plugin repo with `skills/` directory containing three SKILL.md files. Cross-agent support via `AGENTS.md` (for Codex and Amp) and `.claude-plugin/plugin.json` (for Claude Code native discovery). No scripts or dependencies — skills are pure markdown runbooks that agents follow.

**Tech Stack:** Claude Code plugin format, YAML frontmatter, markdown, `gh` CLI commands, `cargo-release`

**Spec:** `docs/superpowers/specs/2026-04-04-jackin-dev-plugin-design.md` in the `jackin-project/jackin` repo.

---

## File Structure

```
jackin-dev/
  .claude-plugin/
    plugin.json          # Claude Code plugin manifest
  .codex/
    INSTALL.md           # Codex setup instructions
  skills/
    release-check/
      SKILL.md           # Pre-release validation skill
    release-notes/
      SKILL.md           # Changelog generation skill
    release/
      SKILL.md           # Release orchestrator skill
  hooks/
    hooks.json           # Claude Code lifecycle hooks (empty for now)
  AGENTS.md              # Cross-agent entry point (Codex, Amp, Claude Code)
  CHANGELOG.md           # Plugin's own changelog
  LICENSE                # Apache-2.0
  README.md              # Plugin overview and installation
```

---

### Task 1: Scaffold the Plugin Repository

**Files:**
- Create: `.claude-plugin/plugin.json`
- Create: `LICENSE`
- Create: `README.md`
- Create: `CHANGELOG.md`
- Create: `AGENTS.md`
- Create: `.codex/INSTALL.md`
- Create: `hooks/hooks.json`

**Prerequisite:** The user has created the `jackin-project/jackin-dev` GitHub repository and cloned it locally.

- [ ] **Step 1: Create the plugin manifest**

Create `.claude-plugin/plugin.json`:

```json
{
  "name": "jackin-dev",
  "description": "Development workflow skills for the jackin project: release management, validation, and changelog generation",
  "version": "0.1.0",
  "author": {
    "name": "Alexey Zhokhov"
  },
  "homepage": "https://github.com/jackin-project/jackin-dev",
  "repository": "https://github.com/jackin-project/jackin-dev",
  "license": "Apache-2.0",
  "keywords": ["release", "changelog", "validation", "cargo-release", "rust"]
}
```

- [ ] **Step 2: Create the LICENSE file**

Create `LICENSE` with the Apache License 2.0 text. Use the full standard Apache-2.0 license body with copyright:

```
Copyright 2025 Alexey Zhokhov
```

- [ ] **Step 3: Create the README**

Create `README.md`:

```markdown
# jackin-dev

Development workflow plugin for the [jackin](https://github.com/jackin-project/jackin) project. Provides skills for release management, validation, and changelog generation.

## Skills

| Skill | Description |
|---|---|
| `release-check` | Pre-release validation: CI status, tests, docs, TODOs, security exceptions |
| `release-notes` | Generate changelog from merged PRs in Keep a Changelog format |
| `release` | Full release orchestrator: check → notes → version → cargo release |

## Installation

### Claude Code

```sh
claude plugin add /path/to/jackin-dev
```

Or add to your project's `.claude/settings.json`:

```json
{
  "plugins": ["/path/to/jackin-dev"]
}
```

### Codex

See [.codex/INSTALL.md](.codex/INSTALL.md).

### Amp Code

Amp reads `AGENTS.md` and discovers skills from `.claude/skills/` compatibility paths. Point Amp at this repo or symlink the skills directory.

## Requirements

These skills are designed for the `jackin-project/jackin` repository and expect:

- `cargo-release` installed (`cargo install cargo-release`)
- `gh` CLI authenticated (`gh auth login`)
- `release.toml` configured with `pre-release-replacements` for CHANGELOG.md
- `.github/workflows/ci.yml` running tests, clippy, and fmt on every push/PR

## License

Apache-2.0
```

- [ ] **Step 4: Create the CHANGELOG**

Create `CHANGELOG.md`:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

### Added
- Initial release with three skills: `release-check`, `release-notes`, `release`
```

- [ ] **Step 5: Create AGENTS.md**

Create `AGENTS.md`:

```markdown
# AGENTS.md

This is a Claude Code plugin providing development workflow skills for the [jackin](https://github.com/jackin-project/jackin) project.

## Available Skills

### release-check

Use when preparing for a release or verifying release readiness.

Runs pre-release validation checks: CI status, local tests, clippy, fmt, doc link validation, TODO freshness, security exceptions review, and Docker build status.

Skill definition: `skills/release-check/SKILL.md`

### release-notes

Use when generating or updating the changelog, preparing release notes, or populating the Unreleased section of CHANGELOG.md.

Generates the `[Unreleased]` section of `CHANGELOG.md` from merged PRs since the last tag, classified into Keep a Changelog categories.

Skill definition: `skills/release-notes/SKILL.md`

### release

Use when performing a release, cutting a new version, or running the full release process.

Orchestrates the full release flow: runs release-check, generates release notes, recommends version, gets user confirmation, then runs `cargo release`.

Skill definition: `skills/release/SKILL.md`

## Requirements

- The target repository must have `cargo-release` configured with `release.toml`
- The target repository must have a `CHANGELOG.md` with `<!-- next-header -->` marker
- The `gh` CLI must be authenticated
- A `.github/workflows/ci.yml` workflow must exist for CI status checks
```

- [ ] **Step 6: Create Codex install instructions**

Create `.codex/INSTALL.md`:

```markdown
# Installing jackin-dev for Codex

Codex does not have a native plugin system. To use these skills:

1. Clone this repo alongside your project:
   ```sh
   git clone https://github.com/jackin-project/jackin-dev.git
   ```

2. Reference the skills in your project's `AGENTS.md`:
   ```markdown
   ## Release Skills
   Release skills are available in `../jackin-dev/skills/`. When asked to run
   release-check, release-notes, or release, read and follow the corresponding
   SKILL.md in that directory.
   ```

3. Alternatively, symlink the skills into your project:
   ```sh
   mkdir -p .codex/skills
   ln -s /path/to/jackin-dev/skills/release-check .codex/skills/release-check
   ln -s /path/to/jackin-dev/skills/release-notes .codex/skills/release-notes
   ln -s /path/to/jackin-dev/skills/release .codex/skills/release
   ```
```

- [ ] **Step 7: Create empty hooks config**

Create `hooks/hooks.json`:

```json
{
  "hooks": {}
}
```

- [ ] **Step 8: Commit the scaffold**

```bash
git add -A
git commit -m "chore: scaffold jackin-dev plugin with manifests and docs"
```

---

### Task 2: Write the `/release-check` Skill

**Files:**
- Create: `skills/release-check/SKILL.md`

- [ ] **Step 1: Create the skill file**

Create `skills/release-check/SKILL.md`:

````markdown
---
name: release-check
description: Use when preparing for a release, verifying release readiness, or running pre-release checks on the jackin project
---

# Release Check

Pre-release validation for the jackin project. Runs a series of checks and produces a readiness report.

## When to Use

- Before running `/release`
- When you want to verify the project is ready to release
- After fixing issues found in a previous release check

## Process

Run each check in order. Collect results into a readiness report.

### Check 1: CI Status

Verify the latest CI run on `main` is green:

```bash
gh run list --workflow=ci.yml --branch=main --limit=1 --json status,conclusion --jq '.[0]'
```

If `conclusion` is not `success`, report **FAIL** and stop.

Also check path-specific workflows if their paths changed since the last tag:

```bash
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

# Check if docker/construct/** changed since last tag
if [ -n "$LAST_TAG" ] && [ -n "$(git diff --name-only "$LAST_TAG"..HEAD -- docker/construct/)" ]; then
  gh run list --workflow=construct.yml --branch=main --limit=1 --json status,conclusion --jq '.[0]'
fi

# Check if docs/** changed since last tag
if [ -n "$LAST_TAG" ] && [ -n "$(git diff --name-only "$LAST_TAG"..HEAD -- docs/)" ]; then
  gh run list --workflow=docs.yml --branch=main --limit=1 --json status,conclusion --jq '.[0]'
fi
```

Report result as **PASS**, **FAIL**, or **SKIP** (if no changes in those paths).

### Check 2: Local Tests

```bash
cargo test --locked
```

If any test fails, report **FAIL** and stop.

### Check 3: Clippy

```bash
cargo clippy -- -D warnings
```

If clippy reports any warnings-as-errors, report **FAIL** and stop.

### Check 4: Format Check

```bash
cargo fmt --check
```

If formatting violations found, report **FAIL** and stop.

### Check 5: Direct Commit Warning

Find commits since the last tag that are not merge commits from PRs:

```bash
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
if [ -n "$LAST_TAG" ]; then
  git log "$LAST_TAG"..HEAD --oneline --no-merges
fi
```

For each commit, check if it was part of a PR:

```bash
gh pr list --state merged --search "<commit-sha>" --json number --jq '.[0].number'
```

If there are commits not associated with any PR, report **WARN** with the list. Do not block.

### Check 6: Doc Link Validation

Check for broken internal links in `docs/src/content/docs/`:

- Read markdown files and extract internal links (relative paths, `href` attributes)
- Verify each linked file exists
- For external URLs, verify they return HTTP 200 (skip rate-limited domains)

Report **PASS** or **WARN** with list of broken links.

### Check 7: TODO Freshness

Read `TODO.md` and all files in `todo/`:

- Check if any items reference work that has already been completed (look for matching commits or closed PRs)
- Check if any items are stale (no related activity in recent commits)

Report **PASS** or **WARN** with findings.

### Check 8: Security Exceptions

Read the "Accepted Exceptions" section in `REVIEW_STATUS.md` and present it to
the user.

Ask: **"Are these security exceptions still current? (yes/no)"**

If the user says no, report **REVIEW** — the user needs to update the file before releasing.

If the user says yes, report **PASS**.

### Check 9: Docker Build Status

Check if Docker-related files changed since the last tag:

```bash
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
CONSTRUCT_CHANGED=$(git diff --name-only "$LAST_TAG"..HEAD -- docker/construct/ 2>/dev/null)
RUNTIME_CHANGED=$(git diff --name-only "$LAST_TAG"..HEAD -- docker/runtime/ 2>/dev/null)
```

If changed, verify the `construct.yml` workflow passed (already checked in Check 1). Report **PASS** or **SKIP**.

## Output

Present a structured readiness report:

```
Release Readiness Report
========================
✓ CI: all workflows green
✓ Local tests: N passed, 0 failed
✓ Clippy: no warnings
✓ Format: clean
⚠ Direct commits: N commits since vX.Y.Z not from PRs
  - <sha> <message>
✓ Doc links: all valid
✓ TODOs: up to date
? Security exceptions: review required (N items)
✓ Docker: builds pass (or: no changes, skipped)

Result: PASS | REVIEW NEEDED | FAIL
```

## Blocking vs Non-blocking

| Check | Failure behavior |
|---|---|
| CI status | **BLOCK** — cannot release with red CI |
| Local tests | **BLOCK** — cannot release with failing tests |
| Clippy | **BLOCK** — cannot release with clippy errors |
| Format | **BLOCK** — cannot release with fmt violations |
| Direct commits | **WARN** — show list, do not block |
| Doc links | **WARN** — show broken links, do not block |
| TODO freshness | **WARN** — show stale items, do not block |
| Security exceptions | **REVIEW** — ask user, block only if user says "no" |
| Docker builds | **BLOCK** if changed and CI failed; **SKIP** if unchanged |
````

- [ ] **Step 2: Commit**

```bash
git add skills/release-check/SKILL.md
git commit -m "feat: add release-check skill"
```

---

### Task 3: Write the `/release-notes` Skill

**Files:**
- Create: `skills/release-notes/SKILL.md`

- [ ] **Step 1: Create the skill file**

Create `skills/release-notes/SKILL.md`:

````markdown
---
name: release-notes
description: Use when generating or updating the changelog, preparing release notes, or populating the Unreleased section of CHANGELOG.md for the jackin project
---

# Release Notes

Generates or updates the `[Unreleased]` section of `CHANGELOG.md` from merged PRs since the last release tag.

## When to Use

- Before a release, to populate the changelog
- When you want to preview what would go into the next release
- When asked to update or regenerate the changelog

## Process

### Step 1: Detect Last Tag

```bash
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
```

If no tags exist, this is the first release. Use the initial commit as the starting point:

```bash
FIRST_COMMIT=$(git rev-list --max-parents=0 HEAD)
```

### Step 2: Get the Tag Date

```bash
TAG_DATE=$(git log -1 --format=%aI "$LAST_TAG" 2>/dev/null || echo "")
```

### Step 3: Gather Merged PRs

```bash
gh pr list --state merged --base main --search "merged:>$TAG_DATE" --json number,title,labels,mergedAt --jq '.[] | {number, title, labels: [.labels[].name], mergedAt}'
```

### Step 4: Classify Each PR

Classify PRs into Keep a Changelog categories based on title prefix and labels:

| Title prefix | Label | Category |
|---|---|---|
| `feat:`, `feature:` | `feature`, `enhancement` | **Added** |
| `fix:` | `bug`, `bugfix` | **Fixed** |
| `security:` | `security` | **Security** |
| `refactor:`, `chore:`, `docs:`, `ci:`, `build:` | `refactor`, `chore`, `docs` | **Changed** |
| `deprecate:` | `deprecated` | **Deprecated** |
| `remove:` | `removed` | **Removed** |

If a PR doesn't match any prefix or label, place it in **Changed** as the default.

Strip the conventional commit prefix from the title when generating the entry. For example:
- `feat: add TUI launcher` becomes `Add TUI launcher`
- `fix: symlink escape in mounts` becomes `Fix symlink escape in mounts`

Capitalize the first letter of each entry.

### Step 5: Find Ungrouped Commits

Find commits since the last tag that are not associated with any merged PR:

```bash
git log "$LAST_TAG"..HEAD --oneline --no-merges
```

For each commit, check if it was part of a PR:

```bash
gh pr list --state merged --search "<commit-sha>" --json number --jq '.[0].number'
```

Commits not associated with a PR go into the **Ungrouped commits** section.

### Step 6: Format the Changelog Section

Format as Keep a Changelog with PR links:

```markdown
## [Unreleased]

### Added
- Add TUI launcher for interactive agent selection ([#12](https://github.com/jackin-project/jackin/pull/12))

### Fixed
- Fix symlink escape in container mounts ([#15](https://github.com/jackin-project/jackin/pull/15))

### Changed
- Extract testing instructions into TESTING.md ([#18](https://github.com/jackin-project/jackin/pull/18))
```

Only include sections that have entries. Do not include empty sections.

If there are ungrouped commits, add:

```markdown
### Ungrouped commits (not from PRs)
- `205875a` docs: fix resolve_agent_source references
- `efa41b8` docs: split TODO into individual files
```

### Step 7: Check for Existing Unreleased Section

Read `CHANGELOG.md` and check if an `[Unreleased]` section already has content.

If it does, ask the user:

> "CHANGELOG.md already has entries in the [Unreleased] section. Do you want to:
> A) Regenerate from scratch (replace existing entries)
> B) Keep existing entries and add any missing PRs"

### Step 8: Write to CHANGELOG.md

If `CHANGELOG.md` does not exist, create it with the standard header:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

[generated entries here]
```

If `CHANGELOG.md` exists, replace the `## [Unreleased]` section content (everything between `## [Unreleased]` and the next `## [` heading or end of file).

### Step 9: Present for Review

Show the complete `[Unreleased]` section to the user. Allow them to:

- Request edits ("move PR #42 to Added", "reword this entry", "remove this commit")
- Approve as-is

Do not proceed until the user approves the changelog content.

## Idempotency

This skill is safe to run multiple times:
- It always checks the current state of `CHANGELOG.md` before writing
- It asks before overwriting existing entries
- PR data is fetched fresh from GitHub each time
````

- [ ] **Step 2: Commit**

```bash
git add skills/release-notes/SKILL.md
git commit -m "feat: add release-notes skill"
```

---

### Task 4: Write the `/release` Skill

**Files:**
- Create: `skills/release/SKILL.md`

- [ ] **Step 1: Create the skill file**

Create `skills/release/SKILL.md`:

````markdown
---
name: release
description: Use when performing a release, cutting a new version, or running the full release process for the jackin project
---

# Release

Full release orchestrator for the jackin project. Runs pre-release validation, generates changelog, recommends version, and executes `cargo release`.

## When to Use

- When you want to cut a new release
- When asked to "release", "cut a version", or "ship it"

## When NOT to Use

- If you just want to check readiness: use `release-check` instead
- If you just want to update the changelog: use `release-notes` instead

## Prerequisites

- `cargo-release` must be installed: `cargo install cargo-release`
- `gh` CLI must be authenticated: `gh auth status`
- `release.toml` must have `pre-release-replacements` configured for CHANGELOG.md
- `CHANGELOG.md` must exist with `<!-- next-header -->` marker
- `.github/workflows/ci.yml` must exist

## Process

### Step 1: Run Release Check

Follow the `release-check` skill completely. Read `skills/release-check/SKILL.md` and execute all checks.

If any **blocking** check fails, **STOP**. Show the readiness report and tell the user what needs to be fixed. Do not proceed.

If only **warnings** or **review items** exist, show the report and ask: "Warnings found. Continue with release? (yes/no)"

### Step 2: Run Release Notes

Follow the `release-notes` skill completely. Read `skills/release-notes/SKILL.md` and execute all steps.

Present the generated changelog section. Allow the user to review and edit.

Do not proceed until the user approves the changelog.

### Step 3: Recommend Version

Read the approved `[Unreleased]` section from `CHANGELOG.md` and analyze the categories:

| Condition | Recommendation |
|---|---|
| Has entries in **Removed** or any entry mentions "breaking" | **major** bump |
| Has entries in **Added** | **minor** bump |
| Only has **Fixed**, **Changed**, **Security**, **Deprecated** | **patch** bump |

Get the current version:

```bash
grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'
```

Calculate the recommended next version and present:

> "Current version: v0.4.0
> Changelog has: 2 Added, 1 Fixed, 1 Changed
> Recommendation: **v0.5.0** (minor — new features added)
>
> Accept this version, or specify a different bump level? (major/minor/patch)"

Wait for user confirmation.

### Step 4: Commit Changelog

If `CHANGELOG.md` has uncommitted changes (from Step 2), commit them:

```bash
git add CHANGELOG.md
git commit -m "docs: update changelog for vX.Y.Z"
```

Replace `X.Y.Z` with the recommended version from Step 3.

### Step 5: Final Confirmation

Present a summary:

```
Release Summary
===============
Version:   v0.5.0 (minor)
Changelog: 2 Added, 1 Fixed, 1 Changed
Checks:    all green (1 warning)
Command:   cargo release minor --execute

This will:
  1. Bump version in Cargo.toml to 0.5.0
  2. Rename [Unreleased] to [0.5.0] - 2026-04-04 in CHANGELOG.md
  3. Add new [Unreleased] section
  4. Create release commit: "chore: release v0.5.0"
  5. Create tag: v0.5.0
  6. Push commit and tag to origin

Proceed? (yes/no)
```

**Do NOT run `cargo release` without explicit "yes" from the user.**

### Step 6: Execute Release

```bash
cargo release {major|minor|patch} --execute
```

Where `{major|minor|patch}` matches the user-confirmed bump level from Step 3.

Monitor the output. If `cargo release` fails, show the error and stop.

### Step 7: Post-Release Verification

Verify the tag was pushed:

```bash
git tag -l "vX.Y.Z"
git ls-remote --tags origin "refs/tags/vX.Y.Z"
```

Remind the user:

> "Release v0.5.0 tagged and pushed. GitHub Actions will now:
> - Build release binaries for all targets
> - Create the GitHub Release with artifacts
> - Update the Homebrew tap
>
> Monitor at: https://github.com/jackin-project/jackin/actions/workflows/release.yml"

## Error Recovery

If something goes wrong at any step:

- **Before `cargo release`:** Safe to fix and re-run `/release`. The skill re-validates everything.
- **During `cargo release`:** Check what was committed/tagged. If the tag was created but not pushed, you can push it manually: `git push origin vX.Y.Z`. If the version bump commit was created but not tagged, you may need to reset and retry.
- **After `cargo release`:** The release is done. If CI fails, check the GitHub Actions workflow.
````

- [ ] **Step 2: Commit**

```bash
git add skills/release/SKILL.md
git commit -m "feat: add release orchestrator skill"
```

---

### Task 5: Apply Prerequisites to the jackin Repository

This task applies changes to the `jackin-project/jackin` repo (not the plugin repo).

**Files:**
- Create: `jackin-project/jackin/.github/workflows/ci.yml`
- Modify: `jackin-project/jackin/release.toml`
- Create: `jackin-project/jackin/CHANGELOG.md`

- [ ] **Step 1: Create the CI workflow**

Create `.github/workflows/ci.yml` in the jackin repo:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
      - run: cargo test --locked
```

- [ ] **Step 2: Update release.toml**

Replace the contents of `release.toml` with:

```toml
allow-branch = ["main"]
publish = false
push = true
tag-name = "v{{version}}"
pre-release-commit-message = "chore: release v{{version}}"
tag-message = "v{{version}}"

pre-release-replacements = [
  { file = "CHANGELOG.md", search = "\\[Unreleased\\]", replace = "[{{version}}] - {{date}}", exactly = 1 },
  { file = "CHANGELOG.md", search = "<!-- next-header -->", replace = "<!-- next-header -->\n\n## [Unreleased]", exactly = 1 },
]
```

- [ ] **Step 3: Create CHANGELOG.md**

Create `CHANGELOG.md` in the jackin repo root:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]
```

- [ ] **Step 4: Commit the prerequisites**

```bash
git add .github/workflows/ci.yml release.toml CHANGELOG.md
git commit -m "chore: add CI workflow, changelog, and cargo-release replacements

Add ci.yml for tests/clippy/fmt on every push and PR.
Add CHANGELOG.md with Keep a Changelog format.
Update release.toml with pre-release-replacements for changelog management."
```

---

### Task 6: Install and Verify the Plugin

- [ ] **Step 1: Install the plugin in Claude Code**

From the jackin project directory:

```bash
claude plugin add /path/to/jackin-dev
```

Or add to `.claude/settings.local.json`:

```json
{
  "plugins": ["/path/to/jackin-dev"]
}
```

- [ ] **Step 2: Verify skill discovery**

Start a new Claude Code session in the jackin project and check that the skills are listed:

- `release-check` should appear in available skills
- `release-notes` should appear in available skills
- `release` should appear in available skills

- [ ] **Step 3: Dry run /release-check**

Invoke the `release-check` skill and verify:

- It checks CI status via `gh run list`
- It runs `cargo test --locked` locally
- It runs `cargo clippy`
- It produces a structured readiness report
- Blocking failures stop the process
- Warnings are shown but don't block

- [ ] **Step 4: Dry run /release-notes**

Invoke the `release-notes` skill and verify:

- It detects the last tag (`v0.4.0`)
- It gathers merged PRs since that tag
- It classifies them into changelog categories
- It identifies ungrouped commits
- It presents the changelog section for review
- It writes to `CHANGELOG.md` after approval

- [ ] **Step 5: Commit verification results**

If any skill files needed adjustments during verification, commit the fixes:

```bash
git add -A
git commit -m "fix: adjust skills based on verification"
```

Only if changes were needed. Skip if everything worked as-is.
