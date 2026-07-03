# Plan 019: Hygiene bundle — dedupe coalesce_cells, fix jackin-runtime's ratatui dep

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-launch-tui/src/tui/components/ crates/jackin-runtime/Cargo.toml`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt / build
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

Two small, verified hygiene items. (1) `coalesce_cells` — the per-cell `(char, Style)` → fewest-`Span`s merger — is implemented three times inside `jackin-launch-tui`; a fix to one silently skips the others. (2) `jackin-runtime` declares `ratatui = "0.30"` as a **normal** dependency but every use is `#[cfg(test)]`-gated — a TUI framework compiles into the non-TUI runtime crate's production graph, and the workspace's dead-dep lints are deliberately off so nothing flags it.

## Current state

Triplicated helper (all three verified present, near-identical bodies):

```rust
// crates/jackin-launch-tui/src/tui/components/progress_rail.rs:226
fn coalesce_cells(cells: impl IntoIterator<Item = (char, Style)>) -> Vec<Span<'static>> { ... }
// crates/jackin-launch-tui/src/tui/components/header.rs:93 — same signature
// crates/jackin-launch-tui/src/tui/components/build_log_dialog.rs:310 — generic form:
fn coalesce_cells<I>(cells: I) -> Vec<Span<'static>>
```

Dep issue:

```toml
# crates/jackin-runtime/Cargo.toml:39 ([dependencies])
ratatui = "0.30"
```
All uses test-only: `crates/jackin-runtime/src/runtime/progress.rs` imports ratatui inside `#[cfg(test)]` blocks (`:55-59` region) and `runtime/progress/tests.rs`. `jackin-tui` is already correctly under `[dev-dependencies]` in the same manifest (`:55`).

Also record (investigation closed, no code change): the console's `save_preview` semantic change-summaries vs the shared `diff_view` text-diff renderer are distinct abstractions — plan 016 Step 0 records the verdict in module docs; nothing to do here.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-launch-tui -p jackin-runtime` then full | pass |

## Scope

**In scope**: the three `coalesce_cells` sites + one new shared home; `crates/jackin-runtime/Cargo.toml`.
**Out of scope**: the animation logic feeding the cells; any other dependency edits; `Cargo.lock` beyond what the manifest change regenerates.

## Git workflow

Branch (operator confirm): `chore/tui-hygiene`. Two commits (`refactor(launch-tui): single coalesce_cells helper`, `build(runtime): move ratatui to dev-dependencies`), `git commit -s`, push each.

## Steps

### Step 1: One `coalesce_cells`

Animated per-cell styling is a general TUI need, and the helper is pure — put it in `crates/jackin-tui/src/components/scrollable_panel.rs`? No — wrong home. Put it in `crates/jackin-tui/src/geometry.rs`? Also no. Decision: `crates/jackin-tui/src/ansi_text.rs` is about parsing, not building. Simplest correct home: a new small `pub fn coalesce_cells` in `crates/jackin-tui/src/components.rs`-adjacent utility — BUT the shared crate should not grow a junk drawer for one intra-crate dedup. **Do the minimal thing**: one `pub(crate) fn coalesce_cells` in a launch-tui-local shared module (e.g. `crates/jackin-launch-tui/src/tui/components/cells.rs` or an existing util module — check what module the three components already share: `rg -n 'mod ' crates/jackin-launch-tui/src/tui/components.rs`), generic form (the `build_log_dialog.rs:310` signature, it subsumes the other two). Delete the two duplicates; import at all three call sites. Promote to `jackin-tui` only when a second crate needs it (documented second-use rule).

**Verify**: `rg -n 'fn coalesce_cells' crates/` → exactly 1; `cargo nextest run -p jackin-launch-tui` → pass.

### Step 2: ratatui → dev-dependencies

In `crates/jackin-runtime/Cargo.toml`, move `ratatui = "0.30"` from `[dependencies]` to `[dev-dependencies]` (merge with the existing dev section that holds `jackin-tui`).

**Verify**: `cargo check -p jackin-runtime` → exit 0 (proves no production use); `cargo nextest run -p jackin-runtime` → pass (test builds still see it); full `cargo nextest run` → pass.

## Test plan

- No new tests. Existing launch-tui render tests cover the helper's three call sites; the compiler proves the dep move.

## Done criteria

- [ ] fmt / clippy / full `cargo nextest run` exit 0
- [ ] `rg -n 'fn coalesce_cells' crates/` → 1
- [ ] `grep -A2 '\[dependencies\]' crates/jackin-runtime/Cargo.toml | grep ratatui` → empty; present under `[dev-dependencies]`
- [ ] `plans/README.md` updated

## STOP conditions

- `cargo check -p jackin-runtime` fails after the move — a production path uses ratatui after all; report the path (the finding was wrong).
- The three `coalesce_cells` bodies turn out to differ semantically (not just in signature) — diff them first; report any behavioral difference instead of silently picking one.

## Maintenance notes

- If a second crate ever needs `coalesce_cells`, promote it to `jackin-tui` then (rule 3 of the reuse canon: consolidate at second use).
- Reviewer: confirm `Cargo.lock` diff is minimal/absent.
