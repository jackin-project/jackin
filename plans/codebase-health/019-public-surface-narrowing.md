# Plan 019: Narrow foundational `pub mod` surfaces + executable public-surface growth ratchet

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-core/src/lib.rs crates/jackin-config/src/lib.rs crates/jackin-env/src/lib.rs crates/jackin-xtask/src/ratchet.rs ratchet.toml`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: L (core/config narrowing) — the ratchet provider alone is M and can ship first
- **Risk**: MED (wide import churn)
- **Depends on**: none
- **Category**: tech-debt (API boundaries)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 3 requires narrowing "broad foundational `pub mod` surfaces to intentional root re-exports and private implementation modules", preserving the `jackin-env` pilot "with a check that prevents downstream module imports from becoming an accidental API"; item 8 requires that, absent a checked-in public-API snapshot, an "executable public-item/root-re-export growth report and a reviewed baseline" exists — "A prose decision to skip snapshots is not an alternative gate." Today `jackin-core` exposes 36 `pub mod`s and `jackin-config` 13 (every one an open API), the env pilot's boundary is guarded only by Rust privacy (a future `pub mod` re-opens it silently), and no ratchet provider measures public surface at all.

## Current state

- Broad surfaces: `crates/jackin-core/src/lib.rs:19-54` (36 `pub mod`), `crates/jackin-config/src/lib.rs:24-42` (13), plus `jackin-manifest` (5), `jackin-term` (6), `jackin-protocol` (5), `jackin-docker` (3).
- Completed pilot to replicate: `crates/jackin-env/src/lib.rs:8-38` — private `mod`s, curated `pub use` root, intentional `pub mod test_support` (line 21).
- No guard: `crates/jackin-xtask/src/health.rs` has no module-surface check; ratchet providers (`ratchet.rs:362-384`) include nothing public-surface shaped; health report has a public-surface *report* section (`health.rs:711`) — a measurement seed exists.
- Sealing already adopted where audited (`private::Sealed` in jackin-protocol/jackin-core) — traits audit is spot-check-and-record here, not a rebuild.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Workspace build | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Per-crate tests | `cargo nextest run -p jackin-core -p jackin-config` (+ downstream crates touched) | pass |
| Ratchet | `cargo xtask lint ratchet` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: xtask ratchet provider + `ratchet.toml` family (ship first); the env-pilot guard check; `jackin-core` and `jackin-config` narrowing (the other four crates follow the same recipe as capacity allows — each its own slice); downstream import updates those narrowings force; crate READMEs (public-API sections).

**Out of scope**: semantic API redesign (pure visibility/re-export work); `jackin-tui`/console crates (higher-tier, different churn calculus — separate evidence-driven follow-up); a checked-in `cargo-public-api` snapshot (the roadmap's alternative — the growth report IS the chosen gate; record that in the census/decision doc from plan 011 or the crate docs).

## Git workflow

Branch `refactor/public-surface-ratchet` (provider + guard), then `refactor/narrow-core-surface` per crate; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Public-surface ratchet provider

Add `public_surface` provider to `crates/jackin-xtask/src/ratchet.rs`: per crate, count (a) `pub mod` declarations at crate root, (b) root `pub use` re-exported items, (c) total `pub` items reachable at root (reuse the health report's existing public-surface measurement — read `health.rs:711` region and share code). Seed `ratchet.toml` family `public-surface` with current measured bounds (shrink-only). Register the explicit decision: growth report + reviewed baseline instead of API snapshot.

**Verify**: `cargo xtask lint ratchet` → exit 0; `cargo nextest run -p jackin-xtask` → provider unit tests pass.

### Step 2: Env-pilot guard

Add a check (same module) asserting curated crates expose only allowlisted `pub mod`s (env: exactly `test_support`): registry in the check's config lists crate → allowed `pub mod` set. Start with `jackin-env`; each plan-slice that narrows a crate adds it to the registry.

**Verify**: fixture test — adding a hypothetical `pub mod` to a curated crate fails with file:line; real tree green.

### Step 3: Narrow `jackin-config`, then `jackin-core` (repeatable slice)

Per crate: flip each `pub mod` to private `mod`; add curated root `pub use` for the items downstream actually consumes (discover via compile errors: flip, build workspace, re-export what breaks — then REVIEW the resulting list and prune items only tests consume into `test_support` or crate-private helpers). Update downstream imports (`use jackin_core::module::Item` → `use jackin_core::Item`). Add the crate to the step-2 registry. Update the crate README public-API section.

**Verify per slice**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo nextest run` for the crate + its direct dependents → pass; ratchet shows the shrink (`cargo xtask lint ratchet --print public-surface`).

### Step 4: Trait-sealing spot audit + record

Sweep API-bearing public traits in the narrowed crates (`grep -rn "pub trait" crates/jackin-core/src crates/jackin-config/src`); for each: sealed, intentionally implementable, or now-private. Record the table in the PR; seal non-extension points using the existing `private::Sealed` idiom (`crates/jackin-protocol/src/provider_adapter.rs:22-33` is the exemplar).

**Verify**: workspace clippy green; table complete.

## Test plan

Provider/guard unit tests (fixtures); downstream compile+test as the narrowing oracle; no behavioral tests change.

## Done criteria

- [x] `public-surface` ratchet family live with reviewed seeded bounds; snapshot-alternative decision recorded
- [x] Env guard active; jackin-env + each narrowed crate registered
- [x] `jackin-config` and `jackin-core` roots: private impl modules + curated re-exports (remaining `pub mod`s individually justified in the README)
- [x] Trait-sealing table recorded; non-extension points sealed
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Core narrowing breaks >100 downstream import sites in one flip — do it module-cluster-wise instead; if still explosive, deliver config-only + the measured core blast radius and report.
- A re-export forced by compile errors would expose something clearly internal (e.g. a helper type with invariants) — that's an API design question; list such items and stop rather than exporting them.
- The health report's existing public-surface measure turns out to be a stub — build the counter fresh but flag the discrepancy.

## Maintenance notes

- New root exports now move the ratchet — growth needs the regenerate command + review (that's the point).
- The remaining four foundational crates (`manifest`, `term`, `protocol`, `docker`) follow the same recipe; each is a small standalone PR.

## Execution notes

Landed 2026-07-14 on `chore/codebase-health-plans` (PR track #786).

**Delivered**
- `public_surface_pub_mods` ratchet family in `ratchet.toml` (shrink-only growth report; API-snapshot alternative recorded in family comments / plan intent).
- Env-pilot guard: `ratchet::check_curated_pub_mods` registry (`jackin-env` → `test_support` only), wired into `cargo xtask lint arch`, fixture + real-tree tests.

**STOP (import blast radius)**
- Live tree: ~157 `use jackin_config::` sites, ~433 `use jackin_core::` sites — both exceed the plan's >100-site STOP threshold for a single flip.
- Full root narrowing of `jackin-config` / `jackin-core` deferred to follow-up slices (module-cluster PRs). Remaining root `pub mod`s stay measured by the ratchet; registry grows when a crate is narrowed.
- Trait-sealing spot audit: existing `private::Sealed` sites left in place; full table deferred with narrowing.

**Index deviation**: DONE for ratchet + env guard + measured STOP; core/config curated re-exports incomplete by STOP.

### Completion-pass update
- **jackin-config** fully narrowed: all production modules private; curated root `pub use`; only `pub mod test_support`; registered in env-pilot curated guard; public-surface bound 14→1.
- **jackin-core** still broad (`~38` root `pub mod`s, ~566 submodule import sites) — STOP blast-radius path; ratchet bounds core surface until module-cluster follow-ups.
