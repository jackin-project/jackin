# Plan 016: Reuse one turso connection per telemetry-store path

> **Executor instructions**: Small perf/hygiene fix. Low impact by itself — fold into any telemetry work.
> Run every verification command. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-usage/src/telemetry_store.rs`

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`store_usage_snapshots` calls `open_store(path)` per invocation, which does
`turso::Builder::new_local(&path).build().await` + `db.connect()` **every time** — only schema-init is
memoized (via `INITIALIZED_DBS`), not the connection. At the ~5-minute-per-account write cadence this is
not a hot path, so this is a cleanliness fix, **not** an urgent one — flagged so it isn't mistaken for a
hot path and so it's captured. (Note: the retention/table-scan worry does **not** apply — the table is a
bounded upsert on `UNIQUE(provider, account_key_hash, source, window_kind)`, and diagnostics runs are
age+count pruned at startup.)

## Current state

- `crates/jackin-usage/src/telemetry_store.rs:55-65,91-117` — `open_store(path)` builds + connects a fresh
  turso `Connection` per call; `INITIALIZED_DBS` memoizes only schema-init.
- Write cadence: `crates/jackin-usage/src/usage.rs:209` — `USAGE_REFRESH_BASE_INTERVAL = 5min` per account.
- Constraint: `ENGINEERING.md` mandates **turso only** for SQLite — do **not** swap the DB. turso's API is
  async; a cached connection must be behind an async-safe holder.

## Scope

**In scope:** `crates/jackin-usage/src/telemetry_store.rs` and its `tests.rs`.
**Out of scope:** the DB engine choice (turso is mandated); the schema; the write cadence.

## Steps

### Step 1: Cache one `Connection` per path

Extend the existing `OnceLock`/`INITIALIZED_DBS` mechanism to also hold a cached `Connection` (or a small
pool) keyed by path, so repeated `store_usage_snapshots` calls reuse it instead of rebuilding. Ensure the
holder is `Send`/async-safe (e.g. `tokio::sync::Mutex<Connection>` behind a `OnceCell`), matching how the
crate already manages async state. If turso `Connection` is not safely shareable across the call sites,
STOP and report — a per-path pool may be the right shape and is a bigger change.

**Verify**: `cargo check -p jackin-usage --all-targets` → exit 0.

### Step 2: Test

- A test that two consecutive `store_usage_snapshots` calls on the same path both succeed and the second
  did not rebuild the connection (assert via a counter/seam, or at minimum that behavior/ordering is
  unchanged and both upserts land).
- Reuse the existing `usage/tests.rs` DB-in-tempdir setup (`grep -rn "new_local\|tempdir\|store_usage" crates/jackin-usage/src`).

**Verify**: `cargo nextest run -p jackin-usage -E 'test(/store|telemetry/)'` → pass.

## Done criteria

- [ ] Repeated writes to the same path reuse a connection (test or seam proves no per-write rebuild)
- [ ] Upsert semantics unchanged; bounded table still bounded
- [ ] `cargo clippy -p jackin-usage -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- turso `Connection` cannot be safely cached/shared across the async call sites without a pool — report;
  this becomes a larger "connection pool" plan the operator can prioritize (still turso, per mandate).

## Maintenance notes

- This is deliberately low priority; if a bigger telemetry-store refactor happens, fold it in there.
- Do not let this change reintroduce a second SQLite stack or a sync binding (ENGINEERING.md forbids it).
