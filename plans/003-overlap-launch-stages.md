# Plan 003: Overlap independent launch stages (secrets, workspace pulls, sidecar, image work)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. Plans 001/006 touch neighboring
> code in `runtime/image.rs` — rebase on them if they landed first.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: none (cleanest after 001)
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

A measured launch (`~/.jackin/data/diagnostics/runs/fec638.jsonl`) runs these
stages strictly one after another:

```
 9.0→ 9.8  role repo_refresh          (0.7s)
 9.8→13.9  workspace git_pull_on_entry (4.2s)
14.1→23.8  image decision              (9.7s, mostly the published pull — Plan 001)
23.8→38.4  credentials operator_env    (14.7s — op:// 1Password reads)
38.6→69.1  agent binaries              (30.5s)
69.4→174.5 docker_build                (105.1s)
174.6→181  role_state + DinD + materialize (~6.4s, these three already overlap each other)
```

`operator_env` (op:// secret reads, observed 3.7–55 s), `git_pull_on_entry`
(observed 3.8–4.2 s), the DinD sidecar bring-up (observed 3–9 s; 28 s with an
image pull), and `role_state_prepare` (0.6–2.4 s) have **no data dependency**
on the image decision or the Docker build, yet none of them overlap the
binaries+build phase. The codebase already proves the pattern is safe: the
sidecar is `select!`-raced with `role_state_prepare` and `join!`-ed with
`materialize_workspace` (`launch_core.rs:662-675`, `:811`). Extending the same
overlap to `operator_env`, the workspace git pull, and by hoisting the sidecar
above the build removes roughly 15–25 s from build launches and 10–20 s from
warm launches, with no ordering-visible behavior change except progress-line
interleaving.

## Current state

- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
  - `git_pull_on_entry` block, lines 666–738: builds `sources`, runs
    `tokio::task::spawn_blocking(pull_git_sources_with_git…)` and **awaits it
    to completion** before `claim_container_name` (line 740–744).
    `pull_git_sources_with_git` itself is already parallel per repo
    (one OS thread per repo, `launch/git_pull.rs:42-86`).
  - Image decision, lines 782–795 (`decide_role_image(...).await?`).
  - `operator_env` resolution, lines 836–901: gated by
    `has_operator_env_matching`, runs `spawn_blocking(resolve_operator_env_matching)`
    and awaits inline. The `jackin_env` layer already parallelizes per key
    (`std::thread::scope`) — the cost is the wall-clock of the slowest `op`
    read (observed up to 55 s; per-read timeout is 120 s,
    `crates/jackin-env/src/op_cli.rs:12`).
  - `manifest_env`, lines 904–935: `resolve_env_with_overrides(&manifest_env,
    &prompter, &operator_env)` — **interactive** (prompts) and consumes
    `operator_env`, so it must stay foreground and after operator_env.
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
  - Image build arm, lines 150–294 (binaries + `build_agent_image`).
  - `adopt_prewarmed_dind_sidecar`, line 296 — after the build.
  - Sidecar future built at lines 555–591, first polled at 662–675
    (select-raced with `role_state_prepare`), joined with materialize at 811.
  - `materialize_workspace` at 783–811; comment at 756–764: "Must run AFTER
    `RoleState::prepare` (so the per-container state directory exists) and
    BEFORE the docker run command".
  - Worktree-leak TODO at 807–810 (`join!` completes materialization after
    sidecar failure) — do not regress it further; fixing it is optional here.

Data-dependency map (verified in code):

| Stage | Needs | Produces |
|---|---|---|
| repo_refresh | network | validated_repo, repo_lock |
| image decision | validated_repo, docker | ImageDecision |
| binaries+build | ImageDecision(Build*), repo_lock | image |
| operator_env | config only (`launch_pipeline.rs:846-856` clones config/selector/workspace) | BTreeMap |
| manifest_env (interactive) | operator_env, validated_repo.manifest | ResolvedEnv |
| git_pull_on_entry | workspace mounts (host paths) | pulled repos (consumed by container at `docker run`) |
| claim_container_name | paths, docker | container_name (needed by role_state, sidecar names) |
| grants/profile resolve | config, manifest | effective_grants (needed by sidecar tier) |
| sidecar | container_name, grants | network+DinD ready |
| role_state_prepare | container_name, manifest, github_ctx | state |
| materialize | role_state done (state dir), container_name | mounts |

Conventions: cancellation must stay wired — long awaits go through
`progress.while_waiting(...)` so Ctrl+C works (see comment at
`launch_core.rs:656-661`); interactive prompts must never interleave with the
build progress surface (why manifest_env stays foreground).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-runtime`                                      | all pass            |
| Full      | `cargo nextest run --all-features`                                         | all pass            |
| E2E       | `cargo nextest run -p jackin --features e2e --profile docker-e2e`          | all pass (run once at the end; needs Docker) |

## Scope

**In scope**:
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
- `crates/jackin-runtime/src/runtime/launch/tests.rs` (+ the pipeline's test
  files under `launch_pipeline/`)
- `docs/content/docs/reference/getting-oriented/architecture.mdx` (launch
  pipeline stage description)

**Out of scope**:
- `decide_role_image` internals (Plan 001), Dockerfile ordering (002),
  repo fetch TTL (004), binary internals (005), DinD prewarm (007).
- The interactive `manifest_env` prompt flow — must remain sequential.
- Fixing the worktree-leak TODO (`launch_core.rs:807`) — separate item; just
  don't make it worse.

## Git workflow

- Branch: `git checkout -b perf/overlap-launch-stages` off `main`.
- Commits per step, each `git commit -s -m "perf(launch): <step>"`; push after
  each commit per CONTRIBUTING.md unless dispatch forbids it.

## Steps

Land as three independent, individually-verifiable sub-changes. After each,
the pipeline must still pass the full test suite.

### Step 1: Start `operator_env` before the image decision; await it where its result is first needed

In `launch_pipeline.rs`:

1. Move the *construction* of the operator-env future (the
   `has_operator_env_matching` gate + `spawn_blocking(resolve_operator_env_matching…)`
   at lines 836–878) to just **before** the `decide_role_image` call (line 784).
   Do not await it there — hold the `JoinHandle`/future.
2. Keep the `active_timing_started("credentials", "operator_env")` mark at
   spawn time and the `…_done` mark where it is awaited, so the diagnostics
   still measure wall-clock (add a detail like `overlapped` on done).
3. Await it exactly where `operator_env` is consumed today (before
   `manifest_env`, line ~879), preserving the error arm (`return Err` with the
   `error` timing detail).
4. Cancellation: the spawned blocking task cannot be aborted mid-`op`-read;
   that matches today's behavior (it is `spawn_blocking` already). The await
   site must keep running under the same progress/cancel wrapper the current
   code uses (it awaits inline today — keep that).

Result: op:// reads overlap the image decision (and, when a build is needed,
the binaries+build phase started in `run_launch_core` — see Step 3 note).

**Verify**: `cargo nextest run -p jackin-runtime` → all pass.

### Step 2: Overlap `git_pull_on_entry` with image work; join before materialize

In `launch_pipeline.rs`:

1. Replace the awaited block at lines 666–738 with: build `sources`, spawn the
   same `spawn_blocking(pull_git_sources_with_git…)`, and stash the join handle
   plus the started timing mark in a local (e.g.
   `let git_pull_join: Option<(JoinHandle<…>, …)>`).
2. Thread the handle into `run_launch_core` (add a field to `LaunchCore` —
   follow the existing pattern of context fields, `launch_core.rs:41-60`), and
   await it **immediately before** `materialize_workspace`
   (`launch_core.rs:783`), emitting the same
   `record_git_pull_results`/`print_git_pull_results` + timing-done +
   progress-stage lines that exist today (move that reporting code with it).
3. Restore semantics: the pulled repos are only consumed by the container
   mounts, and materialize is the first consumer of mount sources — the join
   point guarantees mounts reflect pulled state exactly as today. The restore
   path (`restore_container.is_some()`) skips the pull entirely today
   (line 666) — keep that gate at spawn time.
4. Progress surface: today the Workspace stage shows "polling N workspace
   repositories" during the pull. Keep `stage_started` at spawn; move
   `stage_done` to the join point. The stage will now show as in-progress while
   the image stage advances — that is the intended visible change.

**Verify**: `cargo nextest run -p jackin-runtime` → all pass, including the
fast-restore tests that prove the pull did not run on restore
(`git_program` seam, `launch.rs:126-128`).

### Step 3: Hoist grants + sidecar spawn above the image build arm

In `launch_core.rs`:

1. Move the grant/profile resolution block (currently between the build arm
   and the sidecar construction; find `effective_grants` around lines 316–366)
   to run **before** the image-decision `match` at line 150. It depends only on
   config/manifest/opts. Keep its error handling (`bail_on_grant_errors`)
   exactly as-is — a grant error must still abort before any container-side
   effect.
2. Move `adopt_prewarmed_dind_sidecar` (line 296) and the sidecar-future
   construction (lines 555–591) above the image-decision `match` too, so the
   sidecar (network create → dind image lookup → create → start → TLS wait)
   runs concurrently with binaries+build. Do **not** await it there — the
   existing `select!` with `role_state_prepare` (662–675) and `join!` with
   materialize (811) remain the only await points.
3. The sidecar future needs `container_name`, `network`, `dind`,
   `certs_volume`, `effective_grants` — all available before the build arm
   (container claim happens in `launch_pipeline.rs:740-744`, before
   `run_launch_core`). Confirm by compiler.
4. Failure ordering: today a build failure aborts before the sidecar ever
   starts; after this change a doomed build may leave a started sidecar.
   The existing `cleanup.run(docker)` teardown path (`launch_core.rs:691`)
   already tears down sidecar resources on later failures — extend the build
   arm's error path to run the same `cleanup` before returning (mirror the
   `role_state_result` error arm at 685–693). Verify `LoadCleanup` covers
   network + dind container (see `launch/load_cleanup.rs`; grep `LoadCleanup`
   registrations for `dind`/`network` — if it does not cover them at this
   point in the pipeline, register them when the sidecar future is created).

**Verify**:
- `cargo nextest run -p jackin-runtime` → all pass.
- `cargo nextest run -p jackin --features e2e --profile docker-e2e` → all pass
  (exercises a real launch; requires Docker running).

### Step 4: Docs + gates

Update the launch-pipeline stage narrative in
`docs/content/docs/reference/getting-oriented/architecture.mdx` (stage order /
overlap description).

**Verify**: fmt + clippy + `cargo nextest run --all-features` all green.

## Test plan

- Existing launch pipeline tests must stay green — they encode the ordering
  invariants that matter (trust before build, token verification before DinD,
  claim before summary; see `launch.rs:1-17` doc comment).
- New tests (in the pipeline test files, following existing seams):
  - operator_env overlap: with a slow fake `OpRunner` (the `opts.op_runner`
    seam) assert the image decision proceeds without waiting (e.g. record
    event order via the diagnostics test capture used by existing tests).
  - git-pull join point: with the `git_program` test seam, assert pull results
    are recorded before materialize and that restore still skips the pull.
  - build-failure teardown: build error → sidecar/network cleaned up
    (mock DockerApi call assertions).

## Done criteria

- [ ] fmt/clippy/check green; `cargo nextest run --all-features` passes
- [ ] e2e profile passes locally with Docker
- [ ] Timeline invariants hold in code: operator_env spawned before
      `decide_role_image`; git-pull joined before `materialize_workspace`;
      sidecar future created before the image-build match
- [ ] Interactive `manifest_env` still runs strictly after operator_env await,
      in the foreground
- [ ] No files outside in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- `LaunchCore` field threading requires touching more than the two pipeline
  files plus tests (a sign the seam has moved since planning).
- Grant resolution turns out to depend on the image decision result (re-read
  the block; at planning time it depends only on config/manifest/opts).
- The e2e docker profile fails in a way unit tests do not explain.
- Cleanup coverage for an early-started sidecar cannot be established from
  `LoadCleanup` — report the gap instead of hand-rolling teardown.

## Maintenance notes

- Plan 007 (DinD auto-prewarm) stacks on Step 3: with a prewarmed sidecar,
  `adopt_prewarmed_dind_sidecar` short-circuits the whole sidecar future.
- Reviewer focus: error-path teardown symmetry (every early-spawned resource
  torn down on every abort path), and that no interactive prompt can now
  appear while the rich build surface owns the terminal.
- Deferred explicitly: overlapping `role_state_prepare` with the build (it
  needs `container_name` + `github_ctx` only, so it is possible, but the
  select!-race plumbing with cancel handling is intricate — measure after this
  plan lands; the measured cost is 0.6–2.4 s).
