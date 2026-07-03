# Plan 005: Cached-binary-first agent provisioning; stop network retries from gating launches

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-image/src/agent_binary.rs crates/jackin-image/src/capsule_binary.rs crates/jackin-docker/src/net.rs`
> On mismatch with the excerpts below, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: 001 recommended first (it removes the *decide-path* version
  lookups; this plan fixes the *build-path* provisioning). Executable
  independently.
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

Measured builds spend up to 45 s in a single agent's
`ensure_<agent>_binary` step (`agent binaries/ensure_kimi_binary`: 24.4 s,
26.4 s, 30.5 s, 45.1 s across four recorded runs) — wall-clock for the whole
binaries phase, since agents resolve concurrently and the slowest wins. Two
compounding causes, both in `crates/jackin-image/src/agent_binary.rs`:

1. **Network-before-cache**: once the 1-hour metadata TTL lapses,
   `ensure_available` performs a live "latest release" resolve *before*
   considering the already-downloaded binary on disk; the cached executable is
   only consulted in the error fallback.
2. **Retry stacking** (regression in commit `c3a7fe0f` "fix(agent): retry
   agent binary fetches"): connect-timeouts, previously fail-fast, now retry
   3× with backoff — up to ~46 s per endpoint at 15 s connect timeout, ~93 s
   for Grok's two-endpoint resolve, all serial within one agent's resolve.

Fix: prefer a usable cached binary immediately and demote version resolution to
a bounded, non-gating refresh. Currency is preserved because image staleness is
(after Plan 001) detected by the background sentinel, which can afford the
network time.

## Current state

- `crates/jackin-image/src/agent_binary.rs`
  - `CACHE_TTL: Duration = Duration::from_hours(1)` (line 18), enforced in
    `read_cached_release` (lines 751–759) via `latest.json` mtime.
  - `ensure_available` (lines 65–137): TTL-fresh cache → cached path
    (lines 86–97). TTL-expired → `resolve_latest_release` first (line 103);
    cached executable consulted only on resolve failure
    (`cached_executable_after_failure_async`, lines 106–114).
  - `latest_release` (lines 41–63): same shape (network before newest cached
    release fallback).
  - `fetch_text_with_retry` (lines 467–474): `retry_with_backoff(3, 500ms)`
    around every HTTP GET; `github_latest_asset` (lines 508–528) same for the
    GitHub API; download retry at ~line 604.
  - `github_auth_token` (lines 476–506): spawns `gh auth token` per GitHub-API
    resolve, uncached.
  - Dead code: `runtime_mount_binary_path` (~line 717) — zero callers, doc
    claims a mount-instead-of-bake design that does not exist.
- `crates/jackin-docker/src/net.rs` — `CONNECT_TIMEOUT = 15s`,
  `TEXT_GET_TIMEOUT = 30s`, `DOWNLOAD_TIMEOUT = 5min` (lines 25–31);
  `fetch_text` builds a fresh `reqwest::Client` per call (lines 44–52, 70–71).
- `git show c3a7fe0f` shows the removed connect-timeout fast-fail.
- Consumers: `prepare_agent_binaries`
  (`crates/jackin-runtime/src/runtime/image.rs:1087-1136`) — build path only;
  `needs_agent_update` → `latest_release`
  (`crates/jackin-image/src/version_check.rs:68-86`) — decide path (Plan 001
  moves it to background); `jackin prewarm` CLI.

Behavior contract to preserve (from code comments and tests):
- A resolve failure with **no** cached binary must still fall back to
  `AgentInstall::ScriptFallback` (in-Docker installer) with the operator
  warning (`image.rs:1109-1131`).
- Checksums are verified at download time only; warm path is a stat (audit
  confirmed — do not add per-launch hashing).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-image -p jackin-docker`                       | all pass            |
| Full      | `cargo nextest run --all-features`                                         | all pass            |

## Scope

**In scope**:
- `crates/jackin-image/src/agent_binary.rs` + `agent_binary/tests.rs`
- `crates/jackin-image/src/capsule_binary.rs` + tests (dev-churn item, Step 5)
- `crates/jackin-docker/src/net.rs` + tests (client reuse, timeout knobs)
- `docs/content/docs/reference/getting-oriented/architecture.mdx` only if it
  documents binary provisioning behavior (grep "binaries"; skip if absent)

**Out of scope**:
- `version_check.rs` / decide-path callers (Plan 001).
- The all-supported-agents provisioning policy (design tradeoff, recorded as
  deferred in plans/README.md — do NOT narrow to selected-agent here).
- Wiring the mount-based `runtime_mount_binary_path` design (L effort,
  recorded as deferred) — but DO delete the dead function + stale doc
  (Step 6), which is safe either way.

## Git workflow

- Branch: `git checkout -b perf/binary-cache-first` off `main`.
- `git commit -s` per step (`perf(agent-binary): …`, `fix(net): …`), push after
  commit per CONTRIBUTING.md unless dispatch forbids it.

## Steps

### Step 1: Cached-binary-first in `ensure_available`

Restructure `ensure_available` (agent_binary.rs:65–137):

1. Keep the TTL-fresh branch exactly as today.
2. On TTL-expired: before resolving, look up the newest cached executable
   (`newest_cached_executable_release_async` — the helper already used in the
   error path at line 58). If one exists:
   - Return it immediately (with its recorded version, so image labels stay
     truthful for what is actually installed).
   - Spawn a detached best-effort task (`tokio::spawn`) that runs
     `resolve_latest_release` and, on success,
     `persist_release_cache_async` — refreshing `latest.json` for future
     launches and the background sentinel. Emit the existing
     `agent_binary_resolve_started/resolved` breadcrumbs from that task with a
     `background` detail so diagnostics distinguish it.
3. If no cached executable exists: current behavior unchanged (resolve →
   download → fallback chain).

`latest_release` (used by version_check) keeps its current semantics —
Plan 001 moves its callers off the foreground; do not change it here beyond
what Step 2 does to its transport.

**Verify**: `cargo nextest run -p jackin-image` → all pass (adjust tests that
assert resolve-first ordering; add new ones per Test plan).

### Step 2: Restore connect-timeout fast-fail; bound launch-path retries

In `agent_binary.rs`:

1. Reintroduce the connect-timeout classification removed by `c3a7fe0f`
   (`git show c3a7fe0f` shows the prior `is_connect_timeout(&e) => return Err(e)`
   arms) into `retry_with_backoff`'s callers *for the resolve/metadata path*:
   a connect timeout will not heal within 1.5 s of backoff — fail fast to the
   cached fallback. Keep retries for HTTP 5xx/transient body errors.
2. Keep download retries (they run only when a download is genuinely needed),
   but cap the *resolve* path to `retry_with_backoff(2, 500ms)`.

The commit `c3a7fe0f` was fixing real flakes — preserve retry-on-transient;
only the "endpoint unreachable" class returns to fast-fail. State this in the
commit body.

**Verify**: `cargo nextest run -p jackin-image` → all pass (there are existing
retry tests from c3a7fe0f — update the connect-timeout case to expect
fast-fail + cache fallback).

### Step 3: Reuse one HTTP client

In `net.rs`, memoize the header-less client in a `std::sync::OnceLock` and use
it in `fetch_text`; keep `http_client(headers)` for the authenticated GitHub
path (it already reuses its client across retries).

**Verify**: `cargo nextest run -p jackin-docker` → all pass.

### Step 4: Memoize `gh auth token` per process

In `agent_binary.rs`, cache `github_auth_token()`'s result in a
`tokio::sync::OnceCell` (`Option<String>`): one `gh` subprocess per process
instead of one per GitHub-API resolve. The token's lifetime exceeds any single
launch. (Build-path `resolve_github_token` in
`crates/jackin-runtime/src/runtime/image/version.rs` is out of scope — note it
as deferred if still duplicated.)

**Verify**: `cargo check --all-targets` → exit 0.

### Step 5: Stop per-commit capsule re-downloads for dev builds

In `capsule_binary.rs`: the cache key embeds
`REQUIRED_VERSION = env!("JACKIN_VERSION")` (line 43) =
`<cargo-version>+<git-sha>` — every dev commit misses the cache and re-runs the
preview download + full Sigstore verification. For preview (`-dev`) builds
(`is_preview`, ~line 394), key the cache directory on the cargo version +
`preview` channel instead of the full sha-suffixed string, and accept a cached
preview capsule that verified previously. Release builds keep exact-version
keying. Guard with the existing `JACKIN_CAPSULE_BIN` override untouched.

If, while implementing, you find the preview asset itself is versioned per
commit upstream (i.e. a cached preview capsule would be *wrong*, not just
older), STOP for this step only, record why, and skip it — the step is an
optimization for jackin developers, not operators.

**Verify**: `cargo nextest run -p jackin-image` → all pass.

### Step 6: Delete the dead mount-design function

Remove `runtime_mount_binary_path` (agent_binary.rs ~line 717) and its stale
doc comment claiming binaries are bind-mounted at runtime (they are baked —
`derived_image.rs:357-368`). Zero callers (verify:
`grep -rn runtime_mount_binary_path crates/` → only the definition).

**Verify**: grep returns nothing after removal; `cargo check --all-targets` →
exit 0.

## Test plan

New tests in `agent_binary/tests.rs` (follow the existing async test harness +
fake-server seams there):

- TTL-expired + cached executable present → returns cached path with no
  blocking resolve (assert no `agent_binary_resolve_started` record on the
  calling task / via the recorded breadcrumb sink).
- TTL-expired + no cached executable → resolve path unchanged.
- Connect-timeout during resolve → single attempt, then cache fallback.
- Transient 5xx → still retried.

## Done criteria

- [ ] `ensure_available` returns a cached executable without awaiting any
      network when one exists (test-asserted)
- [ ] Connect-timeout resolve failures no longer retry 3× (test-asserted)
- [ ] `grep -rn runtime_mount_binary_path crates/` → no matches
- [ ] One process-wide reqwest client for `fetch_text`; one `gh auth token`
      spawn per process
- [ ] fmt/clippy/check/`cargo nextest run --all-features` green
- [ ] No files outside in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- The detached refresh task in Step 1 cannot be expressed without leaking into
  test runs (mirror how `spawn_selected_image_refresh` no-ops under
  `#[cfg(test)]`, `runtime/image.rs:745-756` — if that pattern can't apply in
  jackin-image, gate on a runtime flag and report).
- Step 5's preview-asset investigation shows per-commit assets (see the step's
  inline escape hatch).
- Existing c3a7fe0f regression tests encode product intent you'd have to
  delete (rather than adjust) — report instead.

## Maintenance notes

- Staleness detection now lives in the background sentinel (Plan 001): if 001
  is not yet landed, agents can run one release behind until the hourly TTL
  refresh — state this in the PR body so the reviewer sequences merges.
- Reviewer focus: the version string recorded into image labels must reflect
  the binary actually installed (cached version, not the just-resolved one).
- Deferred (README): all-supported-agents provisioning policy (lazy sibling
  provisioning design); duplicate `gh auth token` in
  `runtime/image/version.rs`; mount-based binary delivery design.
