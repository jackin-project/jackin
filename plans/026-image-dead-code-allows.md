# Plan 026: Replace the blanket `#[allow(dead_code)]` cluster in `jackin-image`

> **Executor instructions**: Concrete tech-debt fix that enforces the repo's own suppression rule. Run
> every verification command. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-image/src/image_decision.rs crates/jackin-image/src/image_recipe.rs crates/jackin-image/src/image_build.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`jackin-image` fights the workspace `dead_code = "deny"` lint with a cluster of **blanket
`#[allow(dead_code)]`** on `pub` items that are actually *used cross-crate* (from `runtime/image.rs`,
`launch_core.rs`, `launch_pipeline.rs`). This directly violates the repo's own suppression discipline in
`crates/AGENTS.md`: "Code intentionally unused: `#[expect(dead_code, reason = "…")]`, **never blanket
`#[allow(dead_code)]`**". Worse, these items **aren't unused** — the lint fires because the crate can't see
the cross-crate consumers, so the blanket allows are a lint/visibility mismatch, not a real
"intentionally unused" case. The blanket allows also **mask any genuinely dead item** in these files.

## Current state

- `crates/jackin-image/src/image_decision.rs` — ~10 `#[allow(dead_code, reason = "consumed from runtime/image.rs …")]`
  on `pub` items that are imported cross-crate (verified: `runtime/image.rs`, `launch_core.rs`,
  `launch_pipeline.rs` import from `image_decision`).
- `crates/jackin-image/src/image_recipe.rs` — 3 similar.
- `crates/jackin-image/src/image_build.rs` — 2 similar.
- Rule: `crates/AGENTS.md` — never blanket `#[allow(dead_code)]`; prefer fixing, else `#[expect(..., reason)]`.

## Scope

**In scope:** the three `jackin-image` files above. **Out of scope:** the items' *logic*; the cross-crate
consumers; the workspace lint config (don't turn off `dead_code` — fix the root cause).

## Steps

### Step 1: Diagnose why the lint fires

For each `#[allow(dead_code)]` item, determine the real reason `dead_code` fires despite cross-crate use.
Typical root causes:
- The item is `pub` in a crate where nothing *within* the crate uses it, and the consumers are in another
  crate → the correct fix is often confirming it's genuinely part of the crate's **public API** (then the
  lint shouldn't fire for a truly `pub` re-exported item — check the module's `pub` path and whether it's
  actually reachable/exported from the crate root), or
- The item is used only under a feature/cfg not enabled in the lint build.

Record the actual cause per item (or per cluster) in the row note.

### Step 2: Fix the root cause, remove the blanket allows

- If items are genuinely part of the crate's public API: ensure they're properly exported from the crate
  root (`pub use`), which makes `dead_code` not fire — then delete the `#[allow(dead_code)]`.
- If an item truly is unused within the lint's view but must stay: replace the blanket
  `#[allow(dead_code)]` with a narrow `#[expect(dead_code, reason = "…")]` per the rule (so a future
  genuinely-dead item is still caught).
- If any item turns out to be **actually dead** (no consumer anywhere): delete it (pre-release policy allows
  breaking changes; no migration shim needed).

**Verify**: `cargo clippy -p jackin-image --all-targets --all-features -- -D warnings` → exit 0;
`grep -rn "allow(dead_code" crates/jackin-image/src` → **no matches** (only `expect(dead_code` may remain,
each with a `reason`).

### Step 3: Confirm the whole workspace still builds

Because the fix may change visibility, verify downstream consumers still compile.

**Verify**: `cargo check --workspace --all-targets --all-features` → exit 0;
`cargo nextest run -p jackin-image -p jackin-runtime` → all pass.

## Done criteria

- [ ] `grep -rn "allow(dead_code" crates/jackin-image/src` → no matches
- [ ] Any remaining suppression is `#[expect(dead_code, reason = "…")]` with a real reason
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0 (the CI gate)
- [ ] `cargo check --workspace` exits 0 (downstream consumers still compile)
- [ ] `plans/README.md` row updated

## STOP conditions

- Removing an allow surfaces a genuinely-dead `pub` item that *looks* like it should be used — report it;
  it may be a half-wired feature (a finding), not something to blindly delete.
- The correct fix requires changing the crate's public API surface in a way that ripples widely — report
  before doing a wide visibility change.

## Maintenance notes

- Reviewer: confirm no blanket `#[allow(dead_code)]` returns; the crate should model the rule other crates
  already follow.
- This is a template for any other crate with the same blanket-allow smell — grep the workspace
  (`grep -rn "allow(dead_code" crates`) and note siblings in the row for a possible follow-up.
