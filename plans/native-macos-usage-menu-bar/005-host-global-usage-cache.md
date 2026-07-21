# Plan 005: Unify the host-global usage cache so one refresh feeds every surface

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. If a STOP condition occurs, stop and report rather than improvising a second cache architecture. Update this plan's row in `plans/native-macos-usage-menu-bar/README.md` when complete.
>
> **Drift check (run first)**: `git diff --stat 3c49fff0..HEAD -- crates/jackin-usage crates/jackin-usage-ffi crates/jackin-runtime/src/runtime/launch/launch_runtime.rs crates/jackin/src/cli/usage.rs native/Sources 'docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx' 'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx'`
>
> If an in-scope file changed, compare the "Current state" excerpts below with live code. Any architectural mismatch is a STOP condition.

## Status

- **Priority**: P1 (operator-directed top priority, 2026-07-21)
- **Effort**: L
- **Risk**: MED
- **Depends on**: none (runs before all other plans in this program)
- **Category**: bug / tech-debt / direction
- **Planned at**: commit `3c49fff0`, 2026-07-21

## Why this matters

The operator requires exactly one source of truth for account usage on the host machine: refreshing an account's usage anywhere (menu bar app, host CLI, or any Docker container) must update one shared, host-side cache that every other surface reads immediately, so the same account is never probed in parallel by N containers plus the menu bar, and every surface always shows the same numbers. Today that invariant is broken in two places: (a) containers and host processes coordinate through two **different** host directories, so the menu bar app is an uncoordinated extra prober against the same provider accounts; and (b) a warm process only reads the shared snapshot the *first* time it sees an account, so a refresh performed elsewhere is not visible until the local 5-minute refresh interval elapses. This plan closes both gaps with the existing filesystem coordination layer — no daemon, no new IPC.

## Current state

All building blocks exist; they are just split-brained.

- `crates/jackin-usage/src/usage/refresh.rs:195-211` — library defaults for the three shared coordination dirs point at the **daemon** root:

  ```rust
  pub(crate) fn shared_usage_cooldown_dir() -> PathBuf {
      env_dir_or_home("JACKIN_USAGE_COOLDOWN_DIR", ".jackin/data/daemon/usage-cooldowns")
  }
  pub(crate) fn shared_usage_snapshots_dir() -> PathBuf {
      env_dir_or_home("JACKIN_USAGE_SNAPSHOTS_DIR", ".jackin/data/daemon/usage-snapshots")
  }
  pub(crate) fn shared_usage_lock_dir() -> PathBuf {
      env_dir_or_home("JACKIN_USAGE_LOCK_DIR", ".jackin/data/daemon/usage-locks")
  }
  ```

- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs:913,998-1013` — container launch bind-mounts host `~/.jackin/data/usage-shared` to `/jackin/usage-shared` and overrides the env vars, so **containers** coordinate under `~/.jackin/data/usage-shared/{snapshots,cooldowns,locks}`:

  ```rust
  let usage_shared_dir = paths.jackin_home.join("data").join("usage-shared");
  ...
  let usage_shared_mount = format!("{usage_shared_str}:/jackin/usage-shared");
  ...
  "JACKIN_USAGE_SNAPSHOTS_DIR=/jackin/usage-shared/snapshots",
  "JACKIN_USAGE_COOLDOWN_DIR=/jackin/usage-shared/cooldowns",
  "JACKIN_USAGE_LOCK_DIR=/jackin/usage-shared/locks",
  ```

  Host processes (`HostUsageRuntime` behind the menu bar app and `jackin usage host snapshot`) never set these env vars, so they fall back to `~/.jackin/data/daemon/usage-*`. **The two populations never share snapshots, cooldowns, or locks.**

- `crates/jackin-usage/src/usage.rs:435-464` — shared-snapshot seeding happens only for a `Vacant` in-memory entry (first sight of a target). A newer shared snapshot written later by another process is ignored until the local schedule makes the target due:

  ```rust
  if let std::collections::hash_map::Entry::Vacant(entry) =
      self.snapshots.entry(target.cache_key())
  {
      match read_shared_usage_snapshot(&snapshots_dir, &target.shared_account_key()) {
          Some(view) => { ... entry.insert(CachedUsage { view: stale_shared_view(view, now_epoch()) }); }
          None => ...
      }
  }
  ```

- `crates/jackin-usage/src/usage.rs:640-698` (`mark_refreshed_with_cooldown_dir`) — every successful refresh already writes the per-account shared snapshot and a success cooldown marker valid for one base interval (`USAGE_REFRESH_BASE_INTERVAL`, 5 min at `usage.rs:238-240`); rate-limited errors write a backoff cooldown honoring `Retry-After`.

- `crates/jackin-usage/src/usage/refresh.rs:225-258` — the per-account refresh `flock` (`acquire_account_refresh_lock_in`) is explicitly best-effort (`RefreshLockOutcome::Unavailable` proceeds without a lock). Account keys are identity-scoped: `shared_usage_account_key` at `crates/jackin-usage/src/usage.rs:716-736` hashes the Claude email / Codex `account_id`, so two different logins on one provider never collide, and the same account coordinates across instances.

- `crates/jackin-usage/src/host.rs:194-216` — `HostUsageRuntime` opens per-consumer materializations under `<data_dir>/usage-menu-bar/{snapshots.db,accounts.json}` (`HOST_USAGE_STATE_REL = "usage-menu-bar"`, refresh floor default 300 s). The capsule keeps its own materializations at `/jackin/state/usage/snapshots.db` and `/jackin/run/usage/accounts.json`. These are per-consumer persistence, not coordination — they stay.

- `native/Sources/JackinUsageBridge/PresentationStore.swift:45-49,125-145` — the menu bar app opens `~/.jackin/data` as data dir and polls every 5 s with `refresh(force: false)`; Rust no-ops inside the floor. So once re-seeding (Step 2) lands, the menu bar reflects an external refresh within ≤5 s with zero extra network.

- `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx:35-39` — ADR-011 claims the shared coordination dirs are `~/.jackin/data/daemon/usage-*` "already used by Capsule cross-instance refresh". That is doc/code drift: capsules were repointed to `usage-shared/` at launch time. The ADR must be corrected by this plan.

- Test seams already exist: `acquire_account_refresh_lock_in(dir, key)` and `mark_refreshed_with_cooldown_dir(..., cooldown_dir, snapshots_dir)` take explicit dirs; existing tests live in `crates/jackin-usage/src/usage/tests.rs` — match their style (tempdir-based, no network).

- Repo conventions that apply: comments state non-obvious WHY only; telemetry only through the governed facades already used in this file (`jackin_telemetry::cache::decision`); pre-1.0 latest-only policy means **no migration shim** for the old `daemon/usage-*` files — they are ephemeral cache artifacts and regenerate.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Usage crate tests | `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass |
| Runtime launch tests | `cargo nextest run -p jackin-runtime --locked` | all pass |
| Grep for stragglers | `rg -n "daemon/usage-" crates/ docs/ native/` | only intentional historical mentions (ADR changelog prose), no live path defaults |
| Docs audits | `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` | exit 0 |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope:** `crates/jackin-usage/src/usage.rs`, `crates/jackin-usage/src/usage/refresh.rs`, `crates/jackin-usage/src/usage/tests.rs`, `crates/jackin-usage/src/host.rs` (only if the cooldown-consult fix in Step 3 requires it), `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`, `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx`, roadmap item + overview status lines, this plan/index status.

**Out of scope (do NOT touch):**

- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` — the container mount/env wiring is already correct; the host side moves to it.
- The per-consumer SQLite/JSON materializations (`usage-menu-bar/snapshots.db`, `/jackin/state/usage/snapshots.db`, `accounts.json`) — they persist views for instant paint; the shared JSON snapshots are the coordination truth. Consolidating them is a possible later simplification, not this plan.
- `~/.jackin/data/daemon/accounts.db` (`jackin usage cache accounts` CLI store) and its `--sync-host-cache` flow.
- Any daemon/socket push channel (`jackin❯` daemon is explicitly not a v1 dependency), provider probe logic, DTO/view semantics, Swift sources.

## Git workflow

- Stay on the active feature branch. If execution begins on `main`, propose `fix/usage-shared-cache-unification` and wait for operator confirmation before creating it.
- Signed Conventional Commits (`git commit -s`), push immediately after every commit.
- Do not open or merge a PR unless the operator requests it.

## Steps

### Step 1: Point library defaults at the canonical shared root

In `crates/jackin-usage/src/usage/refresh.rs:195-211`, change the three `env_dir_or_home` defaults from `.jackin/data/daemon/usage-{cooldowns,snapshots,locks}` to `.jackin/data/usage-shared/{cooldowns,snapshots,locks}`. Update the doc comments on these functions and on `env_dir_or_home` (`crates/jackin-usage/src/usage.rs:707-715`) to state the invariant: *one host root, `~/.jackin/data/usage-shared/*`; containers reach the same root via the `/jackin/usage-shared` bind mount; env vars override for tests only*. Fix any test in `crates/jackin-usage/src/usage/tests.rs` that asserts the old default strings.

Env var names and container-side values do not change. Old `daemon/usage-*` files are abandoned, not migrated (ephemeral cache; pre-1.0 latest-only policy).

**Verify**: `rg -n "daemon/usage-" crates/` → no live default remains; `cargo nextest run -p jackin-usage --locked` → all pass.

### Step 2: Re-seed the in-memory cache when the shared snapshot is newer

In `refresh_active_account_snapshots` (`crates/jackin-usage/src/usage.rs:435-464`), extend the seeding pass: for **occupied** entries, read the account's shared snapshot and replace the in-memory view when the shared `fetched_at_epoch` is strictly newer than the in-memory one, wrapping it with the existing `stale_shared_view` helper so status honesty is preserved (an externally fetched view renders as fresh data from cache, never as this instance's own probe). Emit the existing `jackin_telemetry::cache::decision` with `CacheResult::Stale` on replacement, exactly as the vacant path does.

Guard the cost: cache the shared snapshot file's mtime per account key (a small `HashMap<String, SystemTime>` beside `snapshots`) and skip the JSON read when the mtime is unchanged since the last check. The seeding pass runs on every refresh tick (menu bar: every 5 s; capsule: every poll tick), so the steady-state cost must be one `stat` per enabled account per tick.

Propagation contract this step establishes (state it in the function's comment): a refresh completed by any process is visible to every other process within one of its poll ticks, without that process performing network I/O.

**Verify**: new unit test (see Test plan) passes: write a newer shared snapshot into a tempdir via `mark_refreshed_with_cooldown_dir` seams, run a second cache instance's refresh pass with the same dirs, assert its in-memory view updated without any probe running.

### Step 3: Confirm the success cooldown actually suppresses cross-process probing

With one shared root, dedup correctness rests on the cooldown markers (the `flock` at `refresh.rs:225-258` is best-effort and may not propagate across the macOS ↔ Linux-VM boundary on OrbStack/Docker Desktop — treat it as an optimization, never the guarantee). Read `UsageRefreshSchedule::should_refresh` and verify a **success** cooldown marker written by another process defers a due target (not only rate-limit markers). If success markers are not consulted there, wire the check in using the existing `shared_usage_rate_limit_cooldown_active`-style helper as the pattern, keeping forced refreshes (`request_account_refresh` / menu bar Refresh button / `--no-refresh`-inverse paths) able to bypass it exactly as they bypass the floor today.

**Verify**: unit test — instance A `mark_refreshed` writes a success marker; instance B (same shared dirs, its own schedule with the target due) runs the refresh pass and skips the probe; a forced refresh on B still probes.

### Step 4: Correct ADR-011 and the operator guide

- ADR-011 `Host credential roots` section (`docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx:35-39`): replace the three `~/.jackin/data/daemon/usage-*` bullets with `~/.jackin/data/usage-shared/{snapshots,cooldowns,locks}` and one sentence stating the invariant: host processes and containers coordinate through this single root (containers via the `/jackin/usage-shared` mount); at most one prober per account host-wide; every surface reads the same per-account snapshot.
- Operator guide `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx`: in the Settings/Privacy area add one operator-visible sentence — refreshing usage in the menu bar updates the same account snapshot every jackin❯ container reads (and vice versa), so numbers always match across surfaces. No on-disk paths on this user-facing page.
- Roadmap item `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`: mark the "host-global usage cache" open-work item done; sync the overview bullet in `docs/content/docs/roadmap/index.mdx` per the docs discipline rules.

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` → exit 0.

### Step 5: Full regression pass

**Verify**: `cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-runtime --locked` → all pass; `cargo xtask ci --fast` → exit 0.

## Test plan

New tests in `crates/jackin-usage/src/usage/tests.rs`, modeled on the existing tempdir-based tests there (no network; use the `_in`/`_with_cooldown_dir` seams):

1. **Default-root test**: with env vars unset, the three dir resolvers end in `data/usage-shared/{cooldowns,snapshots,locks}` (adjusting the existing default-path assertions).
2. **Re-seed-if-newer**: cache A refreshes (via the seam) into shared dirs; cache B with an older in-memory view for the same account key runs its seeding pass against the same dirs and adopts A's newer view; B performs no probe.
3. **Mtime guard**: unchanged shared file ⇒ second seeding pass performs no JSON read (assert via the mtime map or a counter seam).
4. **Success-cooldown suppression** (Step 3): due target + fresh success marker ⇒ skipped; forced refresh ⇒ probes.
5. **Identity scoping regression**: two different account keys on one surface never read each other's shared snapshot (extend the existing Class III-C test if present rather than duplicating it).

## Done criteria

- [x] Host processes and containers resolve the same host coordination root (`~/.jackin/data/usage-shared/*`); `rg -n "daemon/usage-" crates/` shows no live default.
- [x] A refresh completed in any process is adopted by every other process's cache within one poll tick, without network, proven by test 2.
- [x] Success cooldown markers suppress cross-process duplicate probes; forced refresh still works (test 4).
- [x] ADR-011, operator guide, roadmap item, and overview are truthful about the single-cache invariant.
- [x] `cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-runtime --locked`, docs audits, and `cargo xtask ci --fast` all pass.

## Execution status (honest)

- Implementation complete; live defaults `usage-shared/*`; `adopt_shared_snapshots` present.
- Re-verified 2026-07-21: nextest usage+ffi+runtime **742/742**; docs repo-links/roadmap/research **0**; clippy usage packages **0**; `cargo xtask ci --fast` **exit 0**.

## STOP conditions

- `should_refresh` turns out to already consult success markers in a way that makes Step 3's wiring redundant but tests still show duplicate cross-process probes — the model in "Current state" is wrong somewhere; report the actual mechanism instead of stacking a second gate.
- The re-seed path would need to mutate view semantics (percentages, labels, status enums) to render honestly — DTO changes are out of scope; report.
- Cross-boundary file visibility itself fails (a snapshot written in a container is not observable from the host within a tick on OrbStack/Docker Desktop virtiofs) — that invalidates the filesystem-coordination premise; report with the observed latency, do not start building a socket/daemon channel inside this plan.
- Any fix appears to require touching `launch_runtime.rs` or Swift sources.

## Maintenance notes

This plan makes `~/.jackin/data/usage-shared/*` a load-bearing cross-process contract; future changes to the shared snapshot JSON shape must stay readable by every deployed surface in the same release (single version bump, 5-artifact rule per PRERELEASE policy). The optional later daemon can replace the filesystem layer wholesale, but until then reviewers should reject any new per-surface snapshot store or any probe path that bypasses `mark_refreshed`'s shared write. Deferred follow-ups: consolidating `accounts.db` auto-sync, and folding the per-consumer SQLite materializations into one store.
