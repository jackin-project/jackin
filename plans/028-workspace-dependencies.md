# Plan 028: Adopt `[workspace.dependencies]` and exact-pin `turso`

> **Executor instructions**: Dependency-hygiene migration. Compile-checked at every step. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- Cargo.toml crates/*/Cargo.toml deny.toml`

## Status

- **Result**: DONE — shared workspace dependency table adopted; `turso` exact-pinned
- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (must reconcile per-crate feature sets)
- **Depends on**: none (enables 029, 030)
- **Category**: migration / dx (DEPS-01 + DEPS-02)
- **Planned at**: commit `46511939d`, 2026-07-03

## Completion notes

The root `[workspace.dependencies]` table now owns shared first-party path dependencies plus the repeated external dependencies used across member manifests, including shared test/build dependencies. Member manifests use `workspace = true`, with per-crate feature additions preserved for `tokio`, `serde`, `nix`, `reqwest`, diagnostics OTLP, test-support, and benchmark/snapshot helpers. `turso` is declared once as `=0.7.0-pre.17` with `default-features = false`; both consumers inherit it from the workspace table, and `grep -c "0.7.0-pre" Cargo.lock` stayed at 9.

`deny.toml` now denies `[bans.workspace-dependencies].duplicates`. `cargo deny check bans` still prints the existing transitive duplicate-version warnings that plan 030 owns, but the targeted gate exits successfully and reports no `workspace-duplicate` warnings.

## Why this matters

The root `Cargo.toml` has **no** `[workspace.dependencies]` table, so shared deps are declared per-crate —
`cargo deny check bans` emits **44** `workspace-duplicate` warnings, and reqs have already **drifted** for
the same crate: `tempfile` is `=3.27.0` in ~11 crates but `"3.20"` in five and `"3"` in one; `tokio` is
`=1.52.3` in some, `"1.48"` in `jackin-launch-tui`, `"1"` in others; `clap` `"4.5"` vs `"4"`. Each dep bump
becomes an N-file edit, and the exact-pin-vs-loose mix (e.g. `tempfile "=3.27.0"` beside `"3"`) defeats the
`=` pins. Separately, **`turso`** (the mandated SQLite engine on the telemetry path) is declared as
`"0.7.0-pre.7"` — a **caret** req that matches every `0.7.0-pre.N` with N≥7; the lock has already floated
to `pre.17`, and `renovate.json` runs lockfile maintenance "at any time", so an unattended pass can silently
move the pre-1.0 DB engine (whose on-disk format may change between pre-releases) with no review gate.

## Current state

- Root `Cargo.toml` — no `[workspace.dependencies]` (grep: 0 matches).
- Drift examples (verified): `crates/jackin-host/Cargo.toml:26` `tokio = "1"`, `:33` `tempfile = "3"`;
  `crates/jackin-launch-tui/Cargo.toml:17` `tokio = "1.48"`; `crates/jackin-runtime/Cargo.toml:49`
  `tempfile = "3.20"`, `:57` `tokio = "=1.52.3"` (dev).
- `crates/jackin/Cargo.toml:48` and `crates/jackin-usage/Cargo.toml:32` — `turso = { version = "0.7.0-pre.7", default-features = false }`.
- `deny.toml:108` `multiple-versions = "warn"`; `deny.toml:162 [bans.workspace-dependencies] duplicates = "warn"`
  (the repo is already logging this debt).

## Scope

**In scope:** root `Cargo.toml` (`[workspace.dependencies]`), every member `Cargo.toml` that declares a
shared external or internal `path` crate, `deny.toml` (flip duplicates to deny after). **Out of scope:**
adding/removing actual deps; changing feature *sets* except to reconcile them (see STOP).

## Steps

### Step 1: Hoist shared external deps into `[workspace.dependencies]`

Add a `[workspace.dependencies]` table to root `Cargo.toml` with one entry per shared external crate,
choosing the **highest** currently-required version as the single req (e.g. `tempfile = "=3.27.0"`,
`tokio = "=1.52.3"` with the union of features, `clap = "4.5"`, `anyhow`, `serde_json`, `directories`, …).
For feature-varying deps like `tokio`, put the base in the workspace table and let members add extra
features via `features = [...]` alongside `workspace = true` (`tokio = { workspace = true, features = [...] }`).

### Step 2: Switch members to `dep.workspace = true`

In each member `Cargo.toml`, replace the direct req with `dep = { workspace = true }` (plus any per-crate
extra features). Do this crate-by-crate, running `cargo check -p <crate>` after each to catch a missing
feature immediately.

### Step 3: Exact-pin turso

Set both `turso` entries to `= 0.7.0-pre.17` (the already-resolved lock version) — ideally as a single
`[workspace.dependencies]` entry `turso = { version = "=0.7.0-pre.17", default-features = false }` and
`turso = { workspace = true }` in the two members. This makes future turso bumps a deliberate PR (that runs
the telemetry tests) instead of lockfile-maintenance noise.

### Step 4: Also hoist internal `path` crates (optional but recommended)

Move the `jackin-*` internal `path` deps into `[workspace.dependencies]` too (the 44 duplicates include
internal crates like `jackin-core`×18), so they're declared once.

### Step 5: Tighten the gate

After the hoist, flip `deny.toml` `[bans] multiple-versions` handling and the `[bans.workspace-dependencies]
duplicates` from `warn` to `deny` **for the workspace-duplicate class** so drift can't re-enter. (Leave the
transitive-duplicate `skip` list posture to plan 030 — this step is about first-party workspace duplicates.)

**Verify**: `cargo check --workspace --all-targets --all-features` → exit 0;
`cargo deny check bans` → 0 `workspace-duplicate` warnings;
`grep -c "0.7.0-pre" Cargo.lock` unchanged (turso lock didn't move);
`cargo nextest run -p jackin-usage` → pass (turso path still works).

## Done criteria

- [x] `[workspace.dependencies]` exists; shared external + internal deps declared once
- [x] Every member uses `dep = { workspace = true }` for shared deps; no drifted reqs remain
- [x] `turso` is `=0.7.0-pre.17` in both members (via workspace table)
- [x] `cargo deny check bans` reports 0 `workspace-duplicate` warnings
- [x] `cargo check --workspace --all-targets --all-features` exits 0; `cargo nextest run -p jackin-usage` green
- [x] `plans/README.md` row updated

## STOP conditions

- Reconciling a dep's features breaks a crate that relied on a *narrower* feature set (a feature-unification
  regression) — report the specific crate/feature; don't silently widen features workspace-wide if it pulls
  new transitive trees.
- A member needs a genuinely different version of a crate than the workspace (rare) — leave that one
  per-crate with a comment and note it; don't force a single version that breaks it.

## Maintenance notes

- After this, a dep bump is one edit in `[workspace.dependencies]`. A reviewer should reject new per-crate
  reqs for crates already in the workspace table.
- turso bumps now require a deliberate PR — that PR should run `cargo nextest run -p jackin-usage` and check
  for any on-disk format migration note in turso's changelog (pre-1.0 risk).
- Plan 029 (fs2→fs4) rides on this table; do it right after.
