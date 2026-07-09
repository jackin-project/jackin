# Plan 008: Run docker teardown when a post-success launch finalization step fails

> **Executor instructions**: Follow step by step. Run every verification command
> and confirm the expected result before moving on. If a "STOP condition" occurs,
> stop and report. When done, update the status row in
> `plans/code-health/README.md`.
>
> **Read first**: `crates/jackin-runtime/CLAUDE.md` and the runtime/launch
> behavioral spec at `docs/content/docs/reference/developer-reference/specs/runtime-launch/`
> — the spec is the oracle for launch behavior.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
> If it changed, compare the excerpt below against the live code; on a mismatch,
> treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

`LoadCleanup` reclaims the DinD sidecar container, private network, and certs
volume, but **only** when `cleanup.run(docker)` is called explicitly — it has no
`Drop` impl. In `run_launch_core`, the setup phase carefully calls
`cleanup.run(docker).await` before every early `return Err`. The **finalization
phase** — the steps that run *after* launch succeeds and before the teardown
decision — does not: it uses bare `?` on ~9 fallible calls (status writes,
`inspect_attach_outcome`, `finalize_foreground_session`). If any fails (a `docker
inspect` blip, a disk write error), the function returns `Err` before reaching
the teardown `match` that would reclaim resources, orphaning the DinD
container + network + certs volume. This is inconsistent with the setup phase's
discipline and with the `InspectUnavailable` arm that deliberately keeps
resources; here they leak by omission.

## Current state

`crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`:

- `run_launch_core` returns `anyhow::Result<String>` (`:133`).
- `LoadCleanup::run(&self, docker: &impl DockerApi)` is async, takes `&self`,
  infallible (`load_cleanup.rs:80`).
- The finalization region begins after `cleanup.keep_socket_dir();` (`:1099`) and
  runs to the end of the `ReturnToAgent` block (`:1183`). Every fallible call in
  it uses bare `?`:

  ```rust
  cleanup.keep_socket_dir();                                    // :1099 (infallible)
  super::super::write_instance_status(paths, &container_state, &mut instance_manifest, InstanceStatus::Running)?;   // :1100-1105
  // interactive_finalize, prompt, dirty_exit_policy set up here (:1113-1123)
  let outcome = super::super::inspect_attach_outcome(docker, &container_name).await?;   // :1124
  super::super::write_instance_attach_outcome(...)?;            // :1125-1130
  let mut decision = crate::isolation::finalize::finalize_foreground_session(...).await?;  // :1131-1141
  super::super::write_preserved_status_if_applicable(decision, ...)?;                    // :1142-1147
  if matches!(decision, …ReturnToAgent) {
      start_or_reconnect_capsule_client(...).await?;            // :1158
      let outcome2 = …inspect_attach_outcome(...).await?;       // :1159
      …write_instance_attach_outcome(...)?;                     // :1160-1165
      decision = …finalize_foreground_session(...).await?;      // :1166-1176
      …write_preserved_status_if_applicable(...)?;              // :1177-1182
  }
  // :1200+  teardown `match docker.inspect_container_state(...)` — each arm
  //         decides cleanup.run()/disarm()/keep. This is where cleanup normally happens.
  ```

Key facts that make the fix safe:
- Within the finalization region, `cleanup` is **never** run or disarmed — it
  stays armed. So running it once on the error path is correct and cannot
  double-run.
- Everything created in the region (`interactive_finalize`, `prompt`,
  `dirty_exit_policy`) is used only within it; only `decision` is needed
  afterward (by the teardown match). So the region can be wrapped in a block that
  returns `decision`.
- Tearing down on a finalization error is the safe, consistent action — every
  teardown arm except `InspectUnavailable` already tears down.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Launch tests | `cargo nextest run -p jackin-runtime -E 'test(launch)'` | all pass |
| Crate tests | `cargo nextest run -p jackin-runtime` | all pass |
| Clippy | `cargo clippy -p jackin-runtime --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope**: `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
(only the finalization region wrap).

**Out of scope**:
- The teardown `match` arms (`:1205+`) — those already handle cleanup per arm.
  (A separate, noted follow-up covers the narrower case of a `?` failing *inside*
  a teardown arm after `disarm`/`run`; do not tackle it here.)
- The setup phase (already correct).
- `LoadCleanup` itself — do NOT add a `Drop` impl (it can't be async and lacks a
  `docker` handle; that approach is explicitly out of scope).

## Git workflow

- Branch: operator's active branch, or `fix/launch-finalization-cleanup`.
- One commit, conventional, signed. Example:
  `fix(runtime): tear down DinD when a post-success finalization step fails`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Wrap the finalization region in an error-funnel that runs cleanup

Keep `cleanup.keep_socket_dir();` (`:1099`) where it is. Wrap the region from the
`write_instance_status(... Running)?` call (`:1100`) through the end of the
`ReturnToAgent` `if` block (`:1183`) into an async block that yields `decision`,
and on any error run cleanup then propagate:

```rust
cleanup.keep_socket_dir();

let decision = {
    let finalize_result: anyhow::Result<crate::isolation::finalize::FinalizeDecision> = async {
        super::super::write_instance_status(
            paths, &container_state, &mut instance_manifest, InstanceStatus::Running,
        )?;
        // …the entire existing body from :1107 through :1183, verbatim…
        // (interactive_finalize / prompt / dirty_exit_policy setup, the
        //  inspect_attach_outcome + finalize_foreground_session calls, and the
        //  ReturnToAgent branch). It already ends with `decision` in scope.
        Ok(decision)
    }
    .await;

    match finalize_result {
        Ok(decision) => decision,
        Err(err) => {
            // A post-success finalization step failed. cleanup is still armed
            // (the region never runs/disarms it); reclaim DinD/network/certs
            // rather than orphaning them, consistent with the teardown arms.
            cleanup.run(docker).await;
            return Err(err);
        }
    }
};

// …existing teardown `match docker.inspect_container_state(...)` at :1200 unchanged…
```

Notes for getting this right:
- Move the `let mut decision = …finalize_foreground_session(…)?;` and the
  `ReturnToAgent` block **inside** the async block unchanged; the block's final
  expression is `Ok(decision)`.
- `interactive_finalize`, `prompt`, `dirty_exit_policy` move inside the block
  (they are only used there).
- The async block borrows `paths`, `&mut instance_manifest`, `docker`,
  `container_name`, `config`, `workspace_name`, `runner`, etc., but **not**
  `cleanup`. After `.await`, those borrows release, so `cleanup.run(docker)` and
  the teardown match compile.

**Verify**: `cargo check -p jackin-runtime` exits 0.

### Step 2: Confirm no behavior change on the success path

The success path returns `decision` exactly as before and falls into the same
teardown match. Nothing else changes.

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(launch)'` — all
existing launch tests pass (the happy path and the exit-diagnosis/teardown tests
must be unaffected).

### Step 3: Full check

**Verify**: `cargo nextest run -p jackin-runtime` all pass; `cargo clippy -p
jackin-runtime --all-targets --locked -- -D warnings` exits 0.

## Test plan

- No new test is required for correctness of the wrap (it is a control-flow
  refactor with identical success-path behavior); the existing launch suite is
  the regression guard.
- **Deferred (recorded in README, ties to the "run_launch_core has no direct
  test" finding):** a characterization test using `FakeDockerClient`
  (`runtime/test_support.rs:200`) that forces `inspect_attach_outcome` to error
  during finalization and asserts `cleanup.run` was invoked (e.g. the fake
  records a DinD/network removal). Do this only if the harness makes it
  straightforward; otherwise leave it for the run_launch_core test plan.

## Done criteria

- [ ] `grep -n 'cleanup.run(docker).await;' crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs` shows a call on the finalization error path (in addition to the existing teardown-arm calls)
- [ ] The finalization region is wrapped so a failing `?` in it reaches `cleanup.run` before returning
- [ ] `cargo nextest run -p jackin-runtime` exits 0 (all existing launch tests pass)
- [ ] `cargo clippy -p jackin-runtime --all-targets --locked -- -D warnings` exits 0
- [ ] Only `launch_core.rs` modified (`git status`)
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report if:

- The async-block wrap does not compile because of a borrow conflict you cannot
  resolve by moving a `let` inside/outside the block — report the exact borrow
  error rather than restructuring the teardown match.
- The finalization region no longer matches the excerpt (someone already added a
  guard or extracted the region).
- `cleanup` turns out to be run or disarmed **within** the finalization region
  (`:1100-1183`) after all — then running it again on the error path could
  double-run; report it so the guard can check `armed` first.

## Maintenance notes

- If the launch pipeline is later extracted into typestate phases (the Phase 2
  roadmap direction), this guard becomes a per-phase `Drop`/scope-guard on the
  phase struct; until then this funnel mirrors the setup phase's manual
  discipline.
- Reviewer should trace that every `?` between launch success and the teardown
  decision now funnels through `cleanup.run`, and that the success path is
  byte-for-byte behavior-identical.
- Narrower follow-up (README): the same skip-cleanup-on-`?` pattern can occur
  *inside* individual teardown-match arms after a `disarm`/`run`; assess whether
  those need the same funnel once this lands.
