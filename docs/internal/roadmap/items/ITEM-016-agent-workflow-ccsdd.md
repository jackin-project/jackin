# ITEM-016: Install cc-sdd + remove superpowers plugin

**Phase:** 1  
**Risk:** low-medium  
**Effort:** small (half day)  
**Requires confirmation:** yes  
**Depends on:** ITEM-005 (Starlight Developer Reference for spec location)

## Summary

Replace `obra/superpowers` (the current Claude Code plugin) with `cc-sdd` (gotalab/cc-sdd) — a minimal spec-driven development harness that provides `.claude/commands/spec.md`, `plan.md`, and `execute.md` phase gates. Superpowers is a tool-specific plugin; cc-sdd uses `.claude/commands/` files that are version-controlled in the repo and visible to all agents.

## What superpowers provides today

- Brainstorming skill → spec authoring
- Writing-plans skill → implementation plan
- TDD, debugging, review skills
- Specs stored at `docs/superpowers/specs/`

## cc-sdd equivalent

- `/spec` command → creates a spec MDX page at `docs/src/content/docs/internal/specs/`
- `/plan` command → creates an implementation plan
- `/execute` command → implements against the spec
- No external tooling dependency — just `.claude/commands/*.md` files in the repo

## Steps

1. Install cc-sdd: follow gotalab/cc-sdd setup (creates `.claude/commands/spec.md`, `plan.md`, `execute.md`)
2. Add 5-line section to `AGENTS.md` pointing to cc-sdd and the spec location.
3. Migrate existing `docs/superpowers/specs/` → `docs/src/content/docs/internal/specs/` (review each file; convert shipped features to spec MDX pages).
4. Migrate `docs/superpowers/reviews/` → `docs/src/content/docs/internal/` (historical; keep as archive).
5. Remove superpowers plugin from Claude Code configuration.
6. Test: start a new Claude Code session and verify `/spec` works and no superpowers skills load.

## What needs confirmation

- The exact migration plan for `docs/superpowers/specs/` — which specs are for shipped features (convert to Starlight MDX) vs in-progress (become draft pages)?
- Whether to remove superpowers immediately or run both in parallel for one PR cycle.
