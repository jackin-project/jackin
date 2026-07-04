# Plan 029: Migrate `fs2` → `fs4` (unmaintained-since-2018 file-locking dep)

> **Executor instructions**: Small dependency swap on five crates. Verify locking behavior. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- Cargo.toml crates/jackin/Cargo.toml crates/jackin-runtime/Cargo.toml crates/jackin-instance/Cargo.toml crates/jackin-host/Cargo.toml crates/jackin-usage/Cargo.toml`

## Status

- **Result**: DONE — `fs2` removed; locking migrated to `fs4` 1.1.0
- **Priority**: P2
- **Effort**: S
- **Risk**: LOW-MED (verify lock semantics at each call site)
- **Depends on**: plan 028 (do it as one `[workspace.dependencies]` entry)
- **Category**: migration
- **Planned at**: commit `46511939d`, 2026-07-03

## Completion notes

`fs4 = "1.1.0"` is declared once in `[workspace.dependencies]`; the five former `fs2` consumers inherit it. Current Rust exposes inherent `std::fs::File::lock` / `try_lock`, so former `fs2` lock call sites use explicit `fs4::FileExt` UFCS (`FileExt::lock`, `FileExt::try_lock`, `FileExt::unlock`) to keep the migration pinned to `fs4` semantics. `fs4::available_space` replaces the previous disk-space helper.

Existing concurrent-lock coverage in `runtime::cleanup::tests::prune_instances_reaps_only_unheld_name_locks` verifies that a held exclusive lock blocks a second try-lock. The five-crate test run passed 1490 tests.

## Why this matters

`fs2` (0.4.3, no release since **2018**) is the file-locking dep on five crates. File locking coordinates
concurrent `jackin` processes (runtime/instance/usage), so a future soundness/RUSTSEC issue on an abandoned
crate would have no upstream fix path — and this is a class Renovate/`cargo audit` will **never** surface
(no advisory exists today). The maintained drop-in fork is `fs4` (near-identical API). Cheap maintenance
hedge, best done once via the workspace dep table.

## Current state

`fs2 = "0.4"` in five members (verified):
- `crates/jackin/Cargo.toml:57`
- `crates/jackin-runtime/Cargo.toml:44`
- `crates/jackin-instance/Cargo.toml:32`
- `crates/jackin-host/Cargo.toml:25`
- `crates/jackin-usage/Cargo.toml:25`

`fs2`'s API is the `FileExt` trait (`lock_exclusive`, `try_lock_exclusive`, `unlock`, etc.). `fs4` exposes
the same surface, though the import path / trait name may differ slightly by `fs4` version (it has
`fs4::FileExt` and, in newer versions, sync/async split modules).

## Scope

**In scope:** the five member `Cargo.toml`s (or one `[workspace.dependencies]` entry if plan 028 landed),
and every `use fs2::…` / call site. **Out of scope:** the locking *logic* (semantics must be identical);
adding new locking.

## Steps

### Step 1: Add `fs4`, find the call sites

Add `fs4` (pick the current stable version) to `[workspace.dependencies]` (or the five members). Find all
usages: `grep -rn "fs2\|FileExt\|lock_exclusive\|try_lock\|\.unlock()" crates/*/src`.

### Step 2: Swap imports and calls

Replace `use fs2::FileExt;` with the `fs4` equivalent (`use fs4::fs_std::FileExt;` or `use fs4::FileExt;`
depending on the `fs4` version — check the version's docs via `cargo doc -p fs4 --open` or crates.io). The
method names (`lock_exclusive`, `try_lock_exclusive`, `unlock`) are the same; confirm signatures. Remove
`fs2` from all five members.

**Verify**: `grep -rn "fs2" crates/*/Cargo.toml crates/*/src` → **no matches**;
`cargo check --workspace --all-targets` → exit 0.

### Step 3: Verify locking behavior

Run the tests that exercise concurrent access / locking (find them:
`grep -rln "lock_exclusive\|try_lock\|flock\|advisory" crates/*/src/**/tests.rs`). If a lock call site has
no test, add a minimal one (acquire exclusive lock, assert a second `try_lock_exclusive` from another handle
fails while held).

**Verify**: `cargo nextest run -p jackin -p jackin-runtime -p jackin-instance -p jackin-host -p jackin-usage`
→ all pass; `cargo deny check bans sources` → exit 0 (fs4 is Apache/MIT — confirm license is allowlisted).

## Done criteria

- [x] `grep -rn "fs2" crates` → no matches (dep fully removed)
- [x] `fs4` declared once (workspace table) and used at all former `fs2` sites
- [x] Concurrent-lock behavior verified by test (a held exclusive lock blocks a second try-lock)
- [x] `cargo deny check licenses bans sources` exits 0 (fs4 license accepted)
- [x] `cargo nextest run` green across the five crates
- [x] `plans/README.md` row updated

## STOP conditions

- `fs4`'s API for the version you pick differs enough that a call site's semantics would change (e.g.
  blocking vs non-blocking default) — report; matching semantics exactly is the whole point.
- `fs4`'s license isn't in the `deny.toml` allowlist (Apache-2.0/MIT) — STOP; a license exception is an
  operator decision.

## Maintenance notes

- Reviewer: confirm every swapped call preserves blocking-vs-try and exclusive-vs-shared semantics.
- This removes an abandoned dep with no current advisory — it's a hedge, not a fix; note that so it's not
  deprioritized as "no CVE".
