# Plan 004: Gate the per-launch role-repo `git fetch` behind a freshness TTL and slim the cleanliness check

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-runtime/src/runtime/repo_cache.rs crates/jackin-config/src`
> On mismatch with the excerpts below, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

Every launch — even seconds after the previous one — synchronously runs
`git fetch origin <branch>` + `git merge --ff-only FETCH_HEAD` against the role
repo remote before anything downstream can proceed (measured
`role/repo_refresh`: 0.7–1.2 s on a fast network; unbounded on a slow/offline
one, and the exclusive repo lock is held across the fetch, so concurrent
launches of the same role queue behind it). There is no staleness gate at all.
The same pass also runs
`git status --porcelain --ignored=matching --untracked-files=all` — a full
working-tree walk including ignored files — on every launch. A short TTL
(default 60 s) turns back-to-back launches into zero-network repo resolution
while keeping roles effectively fresh, and `--rebuild`/`--branch`/cold paths
still always fetch.

## Current state

- `crates/jackin-runtime/src/runtime/repo_cache.rs`
  - Lock acquired at lines 397–429 (`lock_file.lock_exclusive()`), held through
    the git section and returned to the caller (released after the build
    context snapshot; see doc comment near line 561).
  - Warm-path git sequence (all unconditional): `remote get-url origin`
    (line ~452), cleanliness gate:

    ```rust
    // repo_cache.rs:~484-500
    let status = runner
        .capture(
            "git",
            &[
                "-C", &repo_path, "status", "--porcelain",
                "--ignored=matching", "--untracked-files=all",
            ],
            None,
        )
        .await?;
    anyhow::ensure!(status.is_empty(), "cached role repo contains local changes or extra files: …");
    ```

    then `git_branch` (rev-parse, line ~506; result discarded when
    `opts.branch_override` is `Some`), then:

    ```rust
    // repo_cache.rs:~518-533
    runner.run("git", &["-C", &repo_path, "fetch", "origin", &branch], None, &git_run_opts).await?;
    let ff_result = runner.run("git", &["-C", &repo_path, "merge", "--ff-only", "FETCH_HEAD"], …).await;
    // ff failure → reset --hard FETCH_HEAD (force-push recovery)
    ```

- Caller: `resolve_agent_repo_with` from
  `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs:418` (timing
  span `role`/`repo_refresh` around it at lines 413–436), and from the prewarm
  paths in `crates/jackin-runtime/src/runtime/image.rs` (`resolve_agent_repo_with`
  at lines 106, 853, 1057) with `RepoResolveOptions::non_interactive()`.
- `RepoResolveOptions` is defined in `repo_cache.rs` (grep
  `struct RepoResolveOptions`) — carries `branch_override`, interactivity,
  debug; extend it here.
- Config crate: `crates/jackin-config/src/` — app-level settings live in
  `AppConfig`; config schema changes must update
  `docs/content/docs/reference/runtime/configuration.mdx` in the same PR
  (PROJECT_STRUCTURE.md cross-reference), and PRERELEASE.md's versioned-file
  rule applies to `config.toml` schema additions (a purely additive optional
  key with a default is the intended cheap case).
- The image decision consumes the repo HEAD SHA (tag = `jk_<role>:<short-sha>`),
  so skipping a fetch means launching from the cached HEAD — exactly what the
  existing `Reuse` fast path does with the image built from that SHA. A stale
  window only delays *picking up new role commits*, never correctness of the
  launched image.

Git identity for freshness: `.git/FETCH_HEAD` mtime is updated by every fetch —
use it as the last-fetch timestamp (no new state file needed; absent file =
never fetched = fetch now).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-runtime -p jackin-config`                     | all pass            |
| Full      | `cargo nextest run --all-features`                                         | all pass            |

## Scope

**In scope**:
- `crates/jackin-runtime/src/runtime/repo_cache.rs` (+ its tests)
- `crates/jackin-config/src/` (one additive optional setting + default + validation)
- Call sites passing `RepoResolveOptions` (launch_pipeline.rs, runtime/image.rs)
  — only to thread the TTL/skip decision
- `docs/content/docs/reference/runtime/configuration.mdx` (new key)
- `docs/content/docs/reference/getting-oriented/architecture.mdx` (role refresh
  behavior, one paragraph)

**Out of scope**:
- Lock-scope narrowing beyond what Step 4 states (full lock redesign deferred).
- The workspace `git_pull_on_entry` (different feature, Plan 003).
- Manifest double-parse cleanup (audit PERF-04 for jackin-manifest — record as
  deferred; trivial but separate).

## Git workflow

- Branch: `git checkout -b perf/role-repo-fetch-ttl` off `main`.
- `git commit -s`, Conventional Commits (`perf(role): …` / `feat(config): …`),
  push after commit per CONTRIBUTING.md unless dispatch forbids it.

## Steps

### Step 1: Add the config knob

Add optional `role_repo_refresh_ttl_seconds: Option<u64>` to the app config
(follow an existing optional top-level setting's pattern in
`crates/jackin-config/src/` — grep an existing `Option<` field in `AppConfig`
for serde attributes + docs style). Default when absent: **60**. `0` means
"always fetch" (today's behavior). Validate non-negative by type.

**Verify**: `cargo nextest run -p jackin-config` → all pass (add a
deserialization test: absent key → None; explicit 0 → Some(0)).

### Step 2: Thread it into `RepoResolveOptions`

Add `refresh_ttl: Option<std::time::Duration>` to `RepoResolveOptions` with a
builder method. Launch call site (`launch_pipeline.rs:418`) fills it from
config (60 s default). Prewarm/background call sites
(`runtime/image.rs:106,853,1057`) pass `Some(Duration::ZERO)` — background
refreshes should always fetch (they exist to pick up new commits).
`opts.rebuild == true` at the launch layer must also force `Duration::ZERO`
(operator asked for fresh — thread `rebuild` where the launch site builds the
options).

**Verify**: `cargo check --all-targets` → exit 0.

### Step 3: Gate the fetch

In the warm-cache branch of `repo_cache.rs`, before the fetch (line ~518):

1. Compute freshness: mtime of `<repo_dir>/.git/FETCH_HEAD`; if the file is
   missing, treat as stale.
2. If `now - mtime < refresh_ttl` (and no `branch_override` mismatch with the
   current branch — if `branch_override` is `Some` and differs from
   `git_branch` output, always fetch): skip `fetch` + `merge`/`reset`, emit
   `jackin_diagnostics::debug_log!("repo_cache", "skipping fetch: last fetch {…}s ago (< ttl)")`
   and set the `repo_refresh` timing detail to `fresh_within_ttl` (the timing
   span lives at the caller — pass the outcome up or emit a compact stage
   detail; simplest: add a `run.compact("repo_refresh_skipped", …)` breadcrumb).
3. Keep the cleanliness gate and `remote get-url` check running in both cases
   (they protect against a corrupted cache regardless of fetch).

Also in this step, two micro-fixes verified in the audit:

- Move the `git_branch` call inside the `map_or_else` `None` arm so it is not
  spawned when `branch_override` is `Some` (today its result is discarded).
- Change the cleanliness gate flags from
  `--ignored=matching --untracked-files=all` to `--untracked-files=normal`
  (drop `--ignored=matching`): the gate exists to protect the pristine-cache
  invariant for `reset --hard` safety; tracked+untracked dirt still trips it,
  ignored files (e.g. editor droppings) no longer walk or abort. If any
  existing test asserts the `--ignored` flag, update it and note the semantic
  in the commit body.

**Verify**: `cargo nextest run -p jackin-runtime` → all pass.

### Step 4: Tests

In `repo_cache`'s test file (grep `mod tests` in repo_cache.rs for the harness;
tests use the `CommandRunner` seam so git calls are recordable):

- TTL fresh → no `fetch` invocation recorded; resolve still returns validated
  repo from cache.
- TTL expired / FETCH_HEAD missing / `refresh_ttl == 0` / rebuild-forced → fetch
  recorded (today's behavior).
- `branch_override` differing from current branch → fetch despite fresh TTL.
- Discarded-`git_branch` fix: with `branch_override` set, no
  `rev-parse --abbrev-ref` call recorded.

**Verify**: `cargo nextest run -p jackin-runtime` → all pass incl. new tests.

### Step 5: Docs + gates

- `configuration.mdx`: document `role_repo_refresh_ttl_seconds` (default 60,
  0 = always fetch; contributor-audience page).
- `architecture.mdx`: one sentence in the role-resolution description.

**Verify**: fmt + clippy + `cargo nextest run --all-features` green.

## Test plan

Covered in Step 4; model after the existing repo_cache tests that stub
`CommandRunner` and assert the exact git argv sequences.

## Done criteria

- [ ] Back-to-back resolve with fresh FETCH_HEAD performs zero `git fetch`
      (test-asserted via runner seam)
- [ ] `rebuild`, branch-override-mismatch, background-prewarm paths always fetch
- [ ] Cleanliness gate no longer passes `--ignored=matching`
- [ ] Config key documented; fmt/clippy/tests green (`--all-features`)
- [ ] No files outside in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- `FETCH_HEAD` turns out not to be written by the fetch invocation used here
  (verify once manually in a scratch clone: `git fetch origin main` then stat
  `.git/FETCH_HEAD`) — if unreliable, switch to a jackin-owned sentinel file
  written after successful fetch, and say so in your report.
- Any consumer depends on launch-time fetch side effects beyond HEAD movement
  (grep for FETCH_HEAD elsewhere in `crates/`).
- The `RepoResolveOptions` threading forces signature changes outside the
  in-scope files.

## Maintenance notes

- The TTL only delays *new role commits* becoming visible to a launch by up to
  60 s (or the operator's setting). Prewarm/background refresh always fetches,
  so a console session converges quickly.
- Reviewer focus: the reset-hard recovery path still only runs after a fetch;
  the skip path must never `reset --hard`.
- Deferred (record in plans/README.md): narrowing the exclusive lock so it is
  not held across the network fetch (concurrency win, not latency-per-launch);
  jackin-manifest double TOML parse + repeated canonicalize (audit PERF-04).
