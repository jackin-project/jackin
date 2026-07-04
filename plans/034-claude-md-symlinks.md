# Plan 034: Restore `CLAUDE.md` symlinks and enforce them with a lint

> **Executor instructions**: Fixes agent-governance drift. Run every verification command. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- CLAUDE.md crates/CLAUDE.md docs/CLAUDE.md .github/CLAUDE.md docker/construct/CLAUDE.md crates/jackin-tui-lookbook/CLAUDE.md`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx / docs
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`AGENTS.md` and `RULES.md` state "CLAUDE.md = symlink to AGENTS.md beside it — recreate:
`ln -s AGENTS.md CLAUDE.md`". In reality **every** `CLAUDE.md` is a regular file, not a symlink, and they
have **drifted**: `.github/AGENTS.md` is 34,355 bytes vs `.github/CLAUDE.md` 31,131 bytes; root `AGENTS.md`
109 lines vs `CLAUDE.md` 97 lines (content differs). This repo is heavily agent-driven — Claude Code reads
`CLAUDE.md` while other tooling reads `AGENTS.md`. The two now carry **different governance**
(merge-authorization, force-push, capsule-smoke mandates), so agent behavior silently depends on which file
its tool loads, and the documented "just recreate the symlink" recovery has never actually applied.

## Current state

- Regular files (not symlinks), sizes confirmed drifted:
  - `./CLAUDE.md` (4.9K) vs `./AGENTS.md` (5.9K)
  - `./.github/CLAUDE.md` (30.4K) vs `./.github/AGENTS.md` (34.3K)
  - also `./crates/CLAUDE.md`, `./docs/CLAUDE.md`, `./docker/construct/CLAUDE.md`,
    `./crates/jackin-tui-lookbook/CLAUDE.md`.
- `AGENTS.md:5` / `RULES.md` document the symlink convention.
- (Vendored `docs/node_modules/**/CLAUDE.md` are out of scope — do not touch.)

## Scope

**In scope:** every first-party `CLAUDE.md` (the six above), and a new lint check (in `jackin-xtask` `lint`
or the hygiene workflow). **Out of scope:** the content of `AGENTS.md` (the symlink makes `CLAUDE.md` follow
it); vendored files under `node_modules`.

## Steps

### Step 1: Decide — symlink (as documented) vs CI-enforced byte-identity

The docs say symlink. Symlinks are the simplest and match the stated convention. **However**, confirm the
docs-site build and any Windows contributor path tolerate symlinked `docs/CLAUDE.md` (git on Windows can
checkout symlinks as text stubs). If symlinks are safe here (they are on macOS/Linux, the stated dev
platforms), use them. If the docs build chokes, fall back to a CI check enforcing byte-identity between each
`CLAUDE.md` and its sibling `AGENTS.md`.

### Step 2: Recreate the symlinks

For each of the six, replace the regular file with a relative symlink to its sibling `AGENTS.md`:
```sh
# from each directory containing an AGENTS.md:
rm CLAUDE.md && ln -s AGENTS.md CLAUDE.md
```
Confirm git records them as symlinks (mode `120000`).

**Verify**: `for f in ./CLAUDE.md ./crates/CLAUDE.md ./docs/CLAUDE.md ./.github/CLAUDE.md ./docker/construct/CLAUDE.md ./crates/jackin-tui-lookbook/CLAUDE.md; do test -L "$f" && echo "symlink: $f" || echo "NOT SYMLINK: $f"; done`
→ all "symlink"; `git ls-files -s CLAUDE.md | grep 120000` → mode 120000.

### Step 3: Add a lint so it can't drift again

Add a check (extend `cargo xtask lint` — the crate already has `lint {files,tests,arch}`) that fails if any
first-party `CLAUDE.md` is **not** a symlink to its sibling `AGENTS.md` (or, if Step 1 chose byte-identity,
that the contents differ). Wire it into the hygiene/CI workflow.

**Verify**: `cargo xtask lint <new-subcheck>` → exit 0 after the symlinks are restored; temporarily
replacing one symlink with a file makes it exit non-zero (then restore).

## Done criteria

- [x] All six first-party `CLAUDE.md` are symlinks to their sibling `AGENTS.md` (mode 120000), or
      byte-identical + CI-enforced if Step 1 chose that
- [x] `.github/CLAUDE.md` now matches `.github/AGENTS.md` (drift gone)
- [x] A lint fails on any future non-symlink/drifted `CLAUDE.md`
- [x] `plans/README.md` row updated

## Completion notes

- Restored the six first-party `CLAUDE.md` files as sibling-relative symlinks to `AGENTS.md`.
- Added `cargo xtask lint agents` and wired it into the umbrella `cargo xtask lint --strict` gate.
- Verified the new lint accepts the restored symlinks and rejects a temporary regular `docs/CLAUDE.md` copy.
- Verified the docs site still builds with symlinked `docs/CLAUDE.md` via `cd docs && mise exec -- bun install --frozen-lockfile && mise exec -- bun run build`.
- Verified the staged symlink entries use git mode `120000`.

## STOP conditions

- The docs-site build (`cd docs && bun run build`) breaks with a symlinked `docs/CLAUDE.md` — fall back to
  the byte-identity CI check for that file and note it.
- git is configured with `core.symlinks=false` on the target env (symlinks become text stubs) — report;
  byte-identity enforcement is the fallback.

## Maintenance notes

- The drift happened because nothing enforced the convention — the Step-3 lint is the real fix; the symlink
  recreation just resolves current drift.
- Reviewer: confirm `.github/CLAUDE.md`'s now-restored content includes the force-push/branch rules that had
  drifted out of it.
