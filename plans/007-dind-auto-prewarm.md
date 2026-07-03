# Plan 007: Auto-prewarm the DinD sidecar in the background; pin the DinD image

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-runtime/src/runtime/launch/launch_dind.rs crates/jackin/src/app/load_cmd.rs crates/jackin/src/cli/prewarm.rs`
> On mismatch with the excerpts below, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none (composes with Plan 003 Step 3)
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

Measured launches pay 3–9 s of DinD sidecar bring-up (`docker_create_dind`
0.9–6.7 s + `wait_dind_ready` 2.1–2.4 s TLS/daemon wait), plus a one-off
**28.7 s `pull_dind_image`** when the image is absent. The codebase already
contains a complete prewarmed-sidecar mechanism — `prewarm_dind_sidecar_container`,
`write_prewarmed_dind_state`, and launch-side `adopt_prewarmed_dind_sidecar`
(adoption is attempted on every launch: `launch_core.rs:296`) — but nothing
feeds it automatically: it only runs when an operator manually invokes
`jackin prewarm --sidecar-container --keep`. Console entry already spawns a
background *image* prewarm (`load_cmd.rs:167-171`, D22); extending the same
hook to keep one sidecar warm makes the sidecar cost ~0 for the common
console-launched session. Separately, `DIND_IMAGE = "docker:dind"` is an
unpinned floating tag — a silent upstream major can change dockerd/TLS behavior
under jackin.

## Current state

- `crates/jackin-runtime/src/runtime/launch/launch_dind.rs`
  - `pub const DIND_IMAGE: &str = "docker:dind";` (line 17).
  - Bring-up sequence `run_dind_sidecar_headless_with_owner` (lines 142–254):
    create_network → `dind_image_lookup` (pull only when tags empty,
    lines 158–183) → create → start → `wait_for_dind` TLS wait (lines 240–252).
  - Prewarm machinery: `prewarm_dind_sidecar_container` (line 260+),
    `write_prewarmed_dind_state`, `adopt_prewarmed_dind_sidecar` (~line 538),
    state file `prewarm-dind.json`, adoption lock
    (`prewarm-dind-adoption.lock` observed in `~/.jackin/data/`), labels
    `LABEL_KIND_PREWARM_DIND`/`LABEL_PREWARM`.
  - Rootless tier note: image + privileged flag come from
    `dind_image_and_privileged(grant)` (lines 150–154) — the rootless tier uses
    `docker:dind-rootless`; find both constants when pinning.
- `crates/jackin/src/app/load_cmd.rs:160-172` — console entry: on Docker
  connect, `play_construct_intro_if_needed` then
  `runtime::spawn_background_image_prewarm(&paths, background_prewarm_targets(&config), debug)`.
- `crates/jackin/src/cli/prewarm.rs` — explicit command; sidecar path at
  ~lines 234–238 calls `prewarm_dind_sidecar_container` and, with `--keep`,
  `write_prewarmed_dind_state`.
- Launch adoption: `launch_core.rs:296`
  `let adopted_sidecar = super::super::adopt_prewarmed_dind_sidecar(paths, docker).await;` —
  adopted state short-circuits the sidecar future (`launch_core.rs:566-569`).
- Measured evidence of the gap: run `fec638` logs
  `prewarmed_dind_adoption … skipped` on every launch.

Conventions: background work must never gate the launch (see D22 comment,
`load_cmd.rs:163-166`); prewarm containers carry the prewarm labels so
GC/cleanup can find them; `renovate.json` manages dependency pinning updates
(check whether a Docker-image custom manager exists for pinned tags — if the
repo has one for `docker/construct/Dockerfile`, register the new pinned tag
there too).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-runtime -p jackin`                            | all pass            |
| E2E       | `cargo nextest run -p jackin --features e2e --profile docker-e2e`          | all pass (needs Docker) |

## Scope

**In scope**:
- `crates/jackin-runtime/src/runtime/launch/launch_dind.rs` (+ tests)
- `crates/jackin-runtime/src/runtime/prewarm_trigger.rs` (+ tests) — new
  sidecar-prewarm spawn helper lives beside the image one
- `crates/jackin/src/app/load_cmd.rs` (console-entry spawn call, one line-ish)
- `crates/jackin-runtime/src/runtime/docker_profile.rs` — only the
  `dind_image_and_privileged` constants if pinning lives there
- `renovate.json` — pin-update automation for the new pinned tag(s), if a
  suitable custom manager pattern already exists
- `docs/content/docs/reference/getting-oriented/architecture.mdx` (sidecar
  lifecycle paragraph)

**Out of scope**:
- Gating `docker run` on sidecar-start instead of TLS-ready (semantics change;
  recorded as deferred — the agent could observe a not-yet-ready docker).
- Poll-interval ramps (Plan 008).
- Plan 003's stage reordering.

## Git workflow

- Branch: `git checkout -b perf/dind-auto-prewarm` off `main`.
- Commits: `perf(dind): auto-prewarm sidecar from console entry` and
  `build(dind): pin docker:dind image tags`; `-s`; push after commit per
  CONTRIBUTING.md unless dispatch forbids it.

## Steps

### Step 1: Pin the DinD image tags

Replace `docker:dind` (and the rootless variant in
`dind_image_and_privileged`) with current major-pinned tags (e.g. `docker:29-dind` /
`docker:29-dind-rootless` — use the latest stable major at implementation
time; check `docker buildx imagetools inspect docker:dind` or Docker Hub).
Add/extend a Renovate custom manager entry so the pin gets update PRs — only
if `renovate.json` already has a datasource=docker pattern to copy; otherwise
record "renovate follow-up" in the PR body and TODO.md.

**Verify**: `grep -rn '"docker:' crates/` shows only pinned tags;
`cargo nextest run -p jackin-runtime` passes (update any test asserting the
literal).

### Step 2: Add `spawn_background_sidecar_prewarm`

In `prewarm_trigger.rs`, add a spawn helper mirroring
`spawn_background_image_prewarm`'s shape (tokio::spawn, never awaited by the
caller, diagnostics breadcrumbs `sidecar_prewarm_started/done/failed`,
`#[cfg(test)]` no-op like `runtime/image.rs:745-756`):

1. If a valid prewarmed-sidecar state already exists (reuse the read/validate
   logic inside `adopt_prewarmed_dind_sidecar` — factor its "read + liveness
   check, without consuming" part into a helper if needed), do nothing.
2. Else run `prewarm_dind_sidecar_container(keep = true)` +
   `write_prewarmed_dind_state` — exactly what
   `jackin prewarm --sidecar-container --keep` does (`cli/prewarm.rs:234-238`);
   reuse those functions, do not duplicate their logic.
3. Concurrency guard: take the same lock the adoption path uses
   (`prewarm-dind-adoption.lock`) in try-lock mode; if held, skip (another
   prewarm or a launch adoption is in flight).
4. Default grant/tier for the prewarmed sidecar: use the same default the
   explicit CLI uses (read `cli/prewarm.rs` for which `DindGrant` it passes) —
   adoption already validates compatibility before adopting; confirm
   `adopt_prewarmed_dind_sidecar` rejects tier mismatches (grep its checks; if
   it does not check tier, add the check there — a rootless launch must not
   adopt a privileged prewarm).

**Verify**: `cargo nextest run -p jackin-runtime` → all pass.

### Step 3: Wire it at console entry

In `load_cmd.rs` next to the image-prewarm spawn (line 167), add the sidecar
spawn. Keep it unconditional-but-cheap: the helper itself no-ops when state
exists or lock is held.

**Verify**: `cargo nextest run -p jackin` → all pass.

### Step 4: Replenish after adoption

A launch that adopts the prewarmed sidecar consumes it. In the launch path,
after successful adoption (`launch_core.rs:296` returns `Some`), spawn the same
helper again (post-attach is fine — add it beside the sibling prewarm spawns in
`launch_runtime.rs:1056-1071`) so the *next* launch also finds one. Thread the
spawn through the existing `SiblingPrewarm`-style context if a direct call is
not reachable from there; keep it fire-and-forget.

**Verify**: `cargo nextest run -p jackin-runtime` → all pass;
`cargo nextest run -p jackin --features e2e --profile docker-e2e` → all pass.

### Step 5: Lifecycle audit + docs

- Confirm GC/prune paths already clean orphaned prewarm sidecars (grep
  `LABEL_KIND_PREWARM_DIND` in `runtime/cleanup.rs` / prune code — the labels
  exist for this; if no GC coverage, add the prewarm container/network/volume
  to the orphan sweep, keeping it out of scope creep: only label-filter
  additions).
- Update `architecture.mdx` sidecar paragraph: sidecar may be pre-created in
  the background and adopted at launch.

**Verify**: fmt + clippy + `cargo nextest run --all-features` green.

## Test plan

- Unit: spawn helper no-ops when state valid / lock held (test seams: state
  file in tempdir paths, as existing launch_dind tests do — grep
  `write_prewarmed_dind_state` in tests for the pattern).
- Unit: adoption tier-mismatch rejection (if added in Step 2.4).
- E2E docker profile: launch adopts a prewarmed sidecar (assert
  `adopt_prewarmed_dind` timing detail != `skipped` after a prewarm; the e2e
  harness in TESTING.md shows how launches are driven).

## Done criteria

- [ ] Console entry spawns sidecar prewarm; launch adopts it (e2e evidence or
      diagnostics assertion)
- [ ] Adopted sidecar replenished post-launch
- [ ] `docker:dind` literals pinned; renovate follow-up recorded if manager absent
- [ ] fmt/clippy/`cargo nextest run --all-features` + e2e profile green
- [ ] No files outside in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- `adopt_prewarmed_dind_sidecar` validation turns out to be name-based in a way
  that a background-created sidecar can't satisfy (read it fully before Step 2).
- No safe default `DindGrant` exists for prewarm (profiles make the tier
  ambiguous) — report; a config-gated prewarm tier is a product decision.
- GC coverage for prewarm leftovers requires more than label-filter additions.

## Maintenance notes

- Interaction with Plan 003 Step 3: with both landed, a cold console launch
  overlaps sidecar bring-up with the build AND usually adopts a warm one.
- Reviewer focus: no orphan accumulation across crash paths (kill -9 between
  create and state-write leaves a labeled container — the GC sweep must own it).
- Deferred (README): lazy TLS-ready gate (start container before dockerd
  ready); rootless-tier prewarm selection.
