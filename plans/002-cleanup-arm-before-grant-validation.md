# Plan 002: Arm `LoadCleanup` immediately after DinD sidecar adoption, before grant validation

> **Executor instructions**: Follow step by step; run every verification command. If a STOP condition
> occurs, stop and report. Update this plan's row in `plans/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 46511939d..HEAD -- crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
> If it changed, compare the "Current state" excerpt against live code; on mismatch, STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`adopt_prewarmed_dind_sidecar` adopts a **running, `--privileged`** DinD container + network + certs
volume and deletes its on-disk prewarm record (so nothing else re-adopts it). Four fallible
grant-validation bails run *after* adoption but *before* `LoadCleanup` is constructed. If any bail
fires (e.g. launch role B whose manifest `min_profile` exceeds the resolved profile, or after an
operator edits grants to an invalid value), B adopts A's privileged sidecar and then returns `Err`
with cleanup not yet armed — orphaning a live privileged container with no on-disk record until the
next command's `gc_orphaned_resources` sweep reaps it. Same cancellation/cleanup class as the deferred
worktree-leak TODO, different resource. The in-code comment at the arming site even claims cleanup is
armed "before any fallible step," which the intervening bails contradict.

## Current state

`crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`, in order of execution:

- `:291` adoption:
  ```rust
  let adopted_sidecar = super::super::adopt_prewarmed_dind_sidecar(paths, docker).await;
  let resources = adopted_sidecar.as_ref().map_or_else(
      || DockerResources::from_container_name(&container_name),
      |sidecar| DockerResources { role_container: container_name.clone(),
          dind_container: Some(sidecar.sidecar.dind.clone()),
          network: sidecar.sidecar.network.clone(),
          certs_volume: Some(sidecar.sidecar.certs_volume.clone()) });
  ```
- `:327`, `:340`, `:354`, `:361` — the fallible bails (all **after** adoption, **before** arming):
  ```rust
  bail_on_grant_errors(grant_errors)?;                         // ~327
  // ...
  if let Some(min) = /* role min_profile */ && !profile_meets_floor(...) {
      anyhow::bail!("role `{}` requires Docker profile `{min}` ...");   // ~340
  }
  // ...
  bail_on_grant_errors(tagged_grant_errors("role", &role_grants))?;      // ~354
  bail_on_grant_errors(tag_errors("merged", validate_effective_grants(&effective_grants)))?; // ~361
  ```
- `:363-376` — the comment claiming "Arm cleanup immediately after adoption, before any fallible step",
  followed by the actual construction:
  ```rust
  let mut cleanup = super::super::LoadCleanup::new(
      container_name.clone(), dind.clone(), certs_volume.clone(), network.clone(), /* ... */);
  ```
  Note `dind`, `certs_volume`, `network` are all computed at `:305-320` from `resources`, i.e. **already
  available before the bails** — so `LoadCleanup::new` can move up without needing later state.

`LoadCleanup::run` is best-effort and idempotent (removing a not-yet-created role container is a no-op),
and for a *fresh* (non-adopted) launch the sidecar isn't started until later, so moving the arming
earlier is safe for both paths.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Build | `cargo check -p jackin-runtime --all-targets` | exit 0 |
| Test | `cargo nextest run -p jackin-runtime -E 'test(/launch/)'` | all pass |
| Clippy | `cargo clippy -p jackin-runtime -- -D warnings` | exit 0 |

## Scope

**In scope:** `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`; a new test in the
matching `.../launch_core/tests.rs` (or the crate's existing launch tests file — locate with
`grep -rl "adopt_prewarmed_dind_sidecar" crates/jackin-runtime/src`).

**Out of scope:**
- The worktree-leak-on-sidecar-fail issue (`TODO.md`, separately tracked) — do not attempt it here.
- `adopt_prewarmed_dind_sidecar` internals and `LoadCleanup`'s teardown logic.
- The grant-validation logic itself — only its *ordering* relative to cleanup arming changes.

## Steps

### Step 1: Move `LoadCleanup::new` to immediately after the resource computation

Relocate the `let mut cleanup = LoadCleanup::new(...)` construction (currently ~`:369-376`) to just
after `dind`/`certs_volume`/`network`/`socket_dir` are computed (~after `:320`) and **before** the first
`bail_on_grant_errors` at `:327`. Keep the explanatory comment with it and update it to state cleanup is
now armed before grant validation.

### Step 2: Route the four grant-validation bails through cleanup

Each `?`/`anyhow::bail!` between the new arming point and the launch proper must now run cleanup before
returning. Follow the pattern the later steps in this same function already use for a cleanup-armed
failure (search downward in `launch_core.rs` for the existing `cleanup.run(docker).await;` +
`return Err(...)` shape and mirror it). Convert each bail to: run `cleanup.run(docker).await;` then
`return Err(...)` with the same message.

**Verify**: `cargo check -p jackin-runtime --all-targets` → exit 0;
`cargo clippy -p jackin-runtime -- -D warnings` → exit 0.

### Step 3: Regression test

Add a test that drives `launch_core` (or the smallest seam that reaches the grant-validation bails) with
(a) an adopted prewarmed sidecar present and (b) a grant/`min_profile` condition that forces a bail, then
asserts the adopted sidecar's container/network/volume teardown was invoked. Use the existing launch
tests as the structural pattern (they already fake `DockerApi` — find one asserting cleanup on a failure
path, e.g. a test named like `*rolls_back*` or `*cleanup*`).

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(/launch/)'` → all pass incl. the new test.

## Done criteria

- [ ] `cargo check -p jackin-runtime --all-targets` exits 0
- [ ] `cargo clippy -p jackin-runtime -- -D warnings` exits 0
- [ ] `LoadCleanup::new` appears **before** the first `bail_on_grant_errors` in `launch_core.rs`
      (`grep -n "LoadCleanup::new\|bail_on_grant_errors" crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs` shows `LoadCleanup::new` at a lower line number than the first `bail_on_grant_errors`)
- [ ] New regression test passes; `cargo nextest run -p jackin-runtime` green
- [ ] Only in-scope files modified
- [ ] `plans/README.md` row updated

## STOP conditions

- The adoption/bail/arm ordering in the excerpt no longer matches (already refactored).
- Moving `LoadCleanup::new` up reveals it needs a value only computed *after* the bails — report what.
- The grant bails already run cleanup (someone fixed it) — then only add the regression test.

## Maintenance notes

- Invariant to preserve: **every** early return between sidecar adoption and the launch proper must run
  `cleanup`. A reviewer should check any newly-added fallible step in that window arms/uses cleanup.
- This is the general fix for the resource-leak-on-early-return class; the still-open worktree-leak TODO
  is the sibling case and could fold into the same cleanup model later.
