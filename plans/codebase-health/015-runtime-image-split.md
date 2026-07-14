# Plan 015: Split `runtime/image.rs` along ownership lines; remove its ratchet file-size exception

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-runtime/src/runtime/image.rs ratchet.toml`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED (core launch path)
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Rust-enforcement item 10: "Remove the runtime image file-size exception by ownership extraction. Split only along named image-resolution/build decisions with characterization coverage, then remove the `ratchet.toml` exception. Do not satisfy the budget with arbitrary line movement." `crates/jackin-runtime/src/runtime/image.rs` is exactly 1941 lines — the only file over the 1850 production cap — held by a grandfathered ratchet entry (`ratchet.toml:20-27`, family `file-size-production`, `key = "crates/jackin-runtime/src/runtime/image.rs"`, `bound = 1941`). The health gate permanently excuses the largest production module until this lands.

## Current state

- `ratchet.toml:25-27` — the exception entry under `[[family]] id = "file-size-production"` with `cap = 1850`.
- `crates/jackin-runtime/src/runtime/image.rs` — 1941 lines. Before designing the split, READ the file's `//!` header and map its responsibilities; expected clusters (verify against the actual code, don't assume): published-image resolution vs locally-built derived image decision; role/base image selection; build orchestration (BuildKit/derived image interplay with `crates/jackin-image`); tag/digest bookkeeping; capsule-binary embedding decisions. The named decisions in the roadmap are "image-resolution/build decisions" — the split boundary is decision ownership, not line count.
- Existing sibling modules to match structurally: `ls crates/jackin-runtime/src/runtime/` (self-named module layout; tests in `<mod>/tests.rs`).
- Characterization: find existing image tests with `ls crates/jackin-runtime/src/runtime/image/ 2>/dev/null` and `grep -rn "mod tests" crates/jackin-runtime/src/runtime/image.rs`. The launch mega-suite (`runtime/launch/tests.rs`) also exercises image paths end-to-end via `FakeDockerClient`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Runtime tests | `cargo nextest run -p jackin-runtime` | pass |
| File-size gate | `cargo xtask lint files` | exit 0 after exception removal |
| Ratchet | `cargo xtask lint ratchet` | exit 0 |
| Lint | `cargo clippy -p jackin-runtime --all-targets -- -D warnings` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-runtime/src/runtime/image.rs` → `image.rs` (root) + new `image/<responsibility>.rs` children + `image/tests.rs` moves; `ratchet.toml` (delete the exception entry); `crates/jackin-runtime/README.md` structure table.

**Out of scope**: behavior changes of any kind (pure extraction); `crates/jackin-image` (separate crate); the launch pipeline decomposition (plan 016) — coordinate if concurrent, both touch runtime.

## Git workflow

Branch `refactor/runtime-image-split`; Conventional Commits; `git commit -s`; push per commit. Extraction commits per responsibility so review maps 1:1 to named decisions.

## Steps

### Step 1: Responsibility map + characterization audit

Read `image.rs` fully. Produce (in the PR description) the decision map: each public fn/entry point → which named decision it serves. For each extraction candidate, verify a characterization test exists (in `image/tests.rs` or the launch suite) covering its observable behavior; write missing characterization tests FIRST (happy path + the decision's branch points), using `FakeDockerClient` fixtures per the existing patterns.

**Verify**: `cargo nextest run -p jackin-runtime` → pass, new characterization tests included.

### Step 2: Extract by named decision

One commit per extraction: move a responsibility cluster into `image/<name>.rs` (e.g. `image/resolution.rs`, `image/build.rs` — final names from the step-1 map), moving its tests into the child's `tests.rs` per the crates/AGENTS.md test-layout rule. `image.rs` root keeps orchestration + re-exports; no semantic edits inside moved code (mechanical moves only; rustfmt applies).

**Verify after each commit**: `cargo nextest run -p jackin-runtime` → pass; `wc -l crates/jackin-runtime/src/runtime/image.rs` shrinking.

### Step 3: Remove the exception

When `image.rs` ≤ 1850 (it should land well under): delete the `[[family.entry]]` block for image.rs from `ratchet.toml`.

**Verify**: `cargo xtask lint files` → exit 0; `cargo xtask lint ratchet` → exit 0; `cargo xtask ci --fast` → exit 0; README structure table updated.

## Test plan

Characterization-first (step 1); the moved tests plus launch-suite runs prove behavior preservation. `cargo nextest run -p jackin-runtime` is the gate after every commit.

## Done criteria

- [ ] `wc -l crates/jackin-runtime/src/runtime/image.rs` ≤ 1850 (and each child reasonably sized)
- [ ] Ratchet exception entry deleted; `cargo xtask lint files` green without it
- [ ] Every extraction commit maps to a named decision (PR description carries the map)
- [ ] No behavior change: full runtime suite green, no test assertions weakened
- [ ] README structure table current; status row updated

## STOP conditions

- The responsibility map shows the file is one tangled decision (extraction would be arbitrary line movement — the thing the roadmap forbids); report the map and stop.
- An extraction forces visibility changes that ripple beyond `runtime/` (pub(crate) leaks) — enumerate; the module boundary may need operator design input.
- Plan 016 landed/in-flight with conflicting moves — rebase and reconcile before continuing.

## Maintenance notes

- The 1850 cap now binds this file; future growth must extract further, not re-add an exception.
- Reviewers: check moved-code diffs with `--color-moved` to confirm mechanical moves.
