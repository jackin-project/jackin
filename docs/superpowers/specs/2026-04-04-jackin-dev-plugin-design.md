# jackin-dev Plugin Design

A Claude Code plugin for simplifying jackin development. Provides skills for release management, with room to grow into other development workflows (testing, security review, Docker construct validation, etc.).

Distributed as a standalone repository: `jackin-project/jackin-dev`.

## Platform Support

| Platform | Discovery mechanism |
|---|---|
| Claude Code | `.claude-plugin/plugin.json`, auto-discovers `skills/`, `agents/`, `hooks/` |
| Codex | `AGENTS.md` references + `.codex/INSTALL.md` setup guide |
| Amp Code | `AGENTS.md` references + reads `.claude/skills/` for compatibility |

All three platforms read `AGENTS.md`, which serves as the universal entry point.

## Repository Structure

```
jackin-dev/
  .claude-plugin/
    plugin.json
  .codex/
    INSTALL.md
  skills/
    release-check/
      SKILL.md
    release-notes/
      SKILL.md
    release/
      SKILL.md
  hooks/
    hooks.json
  AGENTS.md
  CHANGELOG.md
  LICENSE
  README.md
```

### Plugin Manifest

`.claude-plugin/plugin.json`:

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

## Skills

### 1. `/release-check` — Pre-release Validation

**Trigger:** "Use when preparing for a release, verifying release readiness, or running pre-release checks."

**Purpose:** Runs a series of checks and produces a pass/fail readiness report.

**Checks (in order):**

1. **CI status** — Verify all GitHub Actions workflows on `main` are green via `gh run list --workflow=ci.yml --branch=main --limit=1`. Also check `construct.yml` and `docs.yml` if their paths changed since last tag.
2. **Local tests** — Run `cargo test --locked` and `cargo clippy -- -D warnings` locally.
3. **Format check** — Run `cargo fmt --check`.
4. **Direct commit warning** — Scan commits since last tag (`git log <last-tag>..HEAD`) for commits that are not merge commits from PRs. Warn if found, do not block.
5. **Doc link validation** — Check links in `docs/` for broken internal references and dead external URLs.
6. **TODO freshness** — Review `TODO.md` and `todo/*.md` for references to completed work, stale items, or items that should have been resolved.
7. **Security exceptions** — Present the "Accepted Exceptions" section from
   `REVIEW_STATUS.md` and ask: "Are these still current?"
8. **Docker build check** — If `docker/construct/**` or `docker/runtime/**` changed since last tag, verify the corresponding CI workflow passed. If no changes, skip.

**Output:** Structured readiness report with pass/fail/warn/review status per check.

**Behavior:**
- Blocks on critical failures: CI red, tests fail, clippy errors, fmt violations.
- Warnings (direct commits) and review items (security exceptions) are presented but do not block.
- Idempotent: safe to re-run after fixing issues.

### 2. `/release-notes` — Changelog Generation

**Trigger:** "Use when generating or updating the changelog, preparing release notes, or populating the Unreleased section of CHANGELOG.md."

**Purpose:** Generates or updates the `[Unreleased]` section of `CHANGELOG.md` from merged PRs.

**Process:**

1. **Detect last tag** — `git describe --tags --abbrev=0` (e.g., `v0.4.0`).
2. **Gather merged PRs** — `gh pr list --state merged --base main --search "merged:>YYYY-MM-DD"` where the date is the last tag's commit date.
3. **Classify each PR** — From PR title prefix and labels:
   - `feat:` / `feature` label -> **Added**
   - `fix:` / `bug` label -> **Fixed**
   - `security:` -> **Security**
   - `refactor:`, `chore:`, `docs:`, `ci:` -> **Changed**
   - `deprecate:` -> **Deprecated**
   - `remove:` -> **Removed**
4. **Handle direct commits** — Commits not from PRs listed in a separate "Ungrouped commits" section for manual placement or discard.
5. **Format** — Keep a Changelog format with PR links:
   ```markdown
   ## [Unreleased]

   ### Added
   - Add TUI launcher for interactive agent selection ([#12](https://github.com/jackin-project/jackin/pull/12))

   ### Fixed
   - Fix symlink escape in container mounts ([#15](https://github.com/jackin-project/jackin/pull/15))

   ### Ungrouped commits (not from PRs)
   - `205875a` docs: fix resolve_agent_source references
   ```
6. **Idempotency** — If `[Unreleased]` section already exists in `CHANGELOG.md`, ask: "Regenerate from scratch, or edit the existing section?"
7. **Present for review** — Show the generated section. Allow inline edits before writing to file.

**First run (no CHANGELOG.md):** Create the file with the standard Keep a Changelog header, `<!-- next-header -->` marker, and the current `[Unreleased]` section. Do not backfill historical releases.

### 3. `/release` — Orchestrator

**Trigger:** "Use when performing a release, cutting a new version, or running the full release process."

**Purpose:** Orchestrates the full release flow with review gates between each step.

**Flow:**

```
Step 1: Run /release-check
  |
  +-- critical failure -> STOP, show what to fix
  +-- pass (with warnings) -> continue
  |
Step 2: Run /release-notes
  |
  +-- present changelog for review
  +-- allow inline edits
  +-- user approves -> continue
  |
Step 3: Version recommendation
  |
  +-- analyze changelog sections:
  |     has breaking changes or "Removed" -> suggest major
  |     has "Added" -> suggest minor
  |     only "Fixed"/"Changed" -> suggest patch
  +-- show: "Recommend: v0.5.0 (minor) - new features added"
  +-- user confirms or overrides -> continue
  |
Step 4: Commit changelog
  |
  +-- commit CHANGELOG.md with message: "docs: update changelog for vX.Y.Z"
  |
Step 5: Final confirmation
  |
  +-- show summary:
  |     Version: v0.5.0
  |     Changelog: 3 Added, 1 Fixed, 1 Changed
  |     Checks: all green (1 warning)
  |     Command: cargo release minor --execute
  +-- user says go -> continue
  |
Step 6: Run cargo release
  |
  +-- cargo release {major|minor|patch} --execute
  +-- cargo-release handles: version bump in Cargo.toml,
  |   rename [Unreleased] -> [X.Y.Z] - date via pre-release-replacements,
  |   re-add [Unreleased] header, commit, tag, push
  |
Step 7: Post-release
  |
  +-- confirm tag was pushed
  +-- remind: "CI will create GitHub Release + update Homebrew tap"
```

**Key behaviors:**
- Every gate is interactive. Never runs `cargo release` without explicit user confirmation.
- If stopped at any point, re-running `/release` re-validates cheaply. `/release-check` re-runs from scratch (it's just checks). `/release-notes` detects existing `[Unreleased]` section.
- The version suggestion is a recommendation; user has final say.
- `cargo release` handles the actual mechanics (version bump, commit, tag, push). The skill does not duplicate that work.

## Changes Required in the jackin Repository

These changes are prerequisites in the `jackin-project/jackin` repo, not part of the plugin:

### 1. New CI workflow: `.github/workflows/ci.yml`

Triggers on every push to `main` and every PR:

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

### 2. Updated `release.toml`

Add `pre-release-replacements` for changelog management:

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

### 3. New `CHANGELOG.md`

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]
```

## Future Skills (Out of Scope for v0.1.0)

The plugin is designed to accommodate additional jackin development skills:

- `/jackin-dev:test` — Run full test suite with Docker construct validation
- `/jackin-dev:security-review` — Audit security exceptions and scan for new findings
- `/jackin-dev:docs-check` — Validate documentation site builds, links, and freshness
- `/jackin-dev:construct-validate` — Verify Docker construct and runtime images build correctly

These are not part of this design. They are listed to confirm the plugin structure supports growth.
