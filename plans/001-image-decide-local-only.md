# Plan 001: Make the image-freshness decision purely local; move all network staleness checks to the existing background refresh

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-runtime/src/runtime/image.rs crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs crates/jackin-runtime/src/runtime/launch/launch_runtime.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

Measured launch diagnostics (`~/.jackin/data/diagnostics/runs/fec638.jsonl`) show a
**9.5-second untimed gap** on the launch critical path between the
`image_tag_lookup` and `image_recipe` timing events. That gap is
`published_image_is_stale` → `docker.pull_image(published)` — a synchronous
registry pull that runs on **every** launch of any role that declares
`published_image` (the recommended production configuration), even when the
local derived image's recipe matches and will be reused. Additionally, after a
recipe match, `decide_role_image` blocks on `needs_agent_update` for every
supported agent, which performs live HTTPS "latest release" lookups whenever the
1-hour metadata cache has expired (3 retries × 500 ms backoff each on failure;
worst case tens of seconds offline). Both checks only ever influence
*background* refresh scheduling for a reuse launch — they never change which
image the current session runs. Moving them off the foreground path removes
~10 s from every warm launch (more when offline) with zero behavior change to
which image runs now, because the background refresh task re-runs the full
decision itself.

## Current state

Relevant files:

- `crates/jackin-runtime/src/runtime/image.rs` — `decide_role_image` (line 179),
  `published_image_freshness` (line 1778), `published_image_is_stale` (line 1822),
  `spawn_selected_image_refresh` (line 736), `prewarm_agent_image_from_validated_repo`
  (line 886).
- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` — spawns the
  background refresh after attach handoff (lines 1045–1055).
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs` —
  consumes `ImageDecision` (lines 150–294).
- `crates/jackin-image/src/image_decision.rs` — `ImageDecision`,
  `ImageInvalidationReason`, `classify_image_labels` (pure, no Docker calls).
- `crates/jackin-image/src/version_check.rs` — `needs_agent_update` (line 68):
  reads a local per-image version cache, then calls
  `crate::agent_binary::latest_release` (network once its 1-hour TTL lapses).

The problem ordering inside `decide_role_image` (`image.rs`), tags-exist branch:

```rust
// image.rs:278-294 — runs BEFORE the local recipe/label classification:
let mut refresh_reason = None;
if let Some(published) = base_image_override
    && published_image_is_stale(
        published,
        &validated_repo.dockerfile.construct_version,
        head_sha.as_deref(),
        docker,
    )
    .await
{
    ...
    base_image_override = None;
    refresh_reason = Some(ImageInvalidationReason::PublishedImageStale);
}
```

`published_image_freshness` starts with an unconditional registry pull:

```rust
// image.rs:1784
if let Err(e) = docker.pull_image(published).await {
```

After the recipe matches (`classify_image_labels(...) == None`), the
agent-version network check still runs in the foreground:

```rust
// image.rs:359-370
let agents = validated_repo.manifest.supported_agents();
let image_ref = &image;
let checks = agents.iter().map(|&agent| async move {
    (
        agent,
        version_check::needs_agent_update(paths, image_ref, agent).await,
    )
});
let results = futures_util::future::join_all(checks).await;
```

Both network stalls are **not wrapped** in
`jackin_diagnostics::active_timing_started/done` (contrast `image_tag_lookup`
at `image.rs:209` and `image_label_inspect` at `image.rs:311`), so they are
invisible in run diagnostics.

The background refresh task already exists and re-runs the whole decision:
`spawn_selected_image_refresh` (`image.rs:736`) → `prewarm_agent_image` →
`decide_role_image` again → builds if needed. It is currently spawned only when
the foreground decision already returned `RefreshInBackground`
(`launch_runtime.rs:1045-1055`, gated on `ctx.selected_image_refresh`).

Repo conventions that apply:

- Two-tier telemetry: `jackin_diagnostics::debug_log!` for firehose,
  `emit_compact_line`/`tracing::warn!` for always-on operator-visible
  degradation (see the existing tag-lookup fallback at `image.rs:231-239`).
- Comments explain WHY only (ENGINEERING.md).
- Tests live in `crates/jackin-runtime/src/runtime/image/…` and
  `crates/jackin-runtime/src/runtime/launch/tests.rs`; module tests always in a
  single `tests.rs`, never inline `mod tests { … }` bodies in source files
  (crates/AGENTS.md).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-runtime -p jackin-image`                      | all pass            |
| Full tests| `cargo nextest run --all-features`                                         | all pass            |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-runtime/src/runtime/image.rs`
- `crates/jackin-runtime/src/runtime/image/tests.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` (only the
  `selected_image_refresh` spawn condition)
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs` /
  `launch_pipeline/launch_core.rs` (only if the `SelectedImageRefresh` wiring
  type needs a new reason variant threaded through)
- `crates/jackin-image/src/image_decision.rs` + its `tests.rs` (only if a new
  `ImageInvalidationReason` variant or decision variant is needed)
- `docs/content/docs/reference/getting-oriented/architecture.mdx` (behavior of
  the launch-time image decision — update the description of when the published
  image is pulled; PROJECT_STRUCTURE.md maps `jackin-runtime` container-lifecycle
  changes to this page)

**Out of scope** (do NOT touch, even though they look related):
- `crates/jackin-image/src/agent_binary.rs` retry/TTL internals — Plan 005.
- Dockerfile generation / layer ordering — Plan 002.
- `--pull` / cache-bust behavior inside `build_agent_image` — Plan 006.
- The DinD sidecar and any launch-stage reordering — Plans 003/007.

## Git workflow

- Branch off `main`: `git checkout -b feature/image-decide-local-only`
  (never commit to `main` — repo hard rule).
- Conventional Commits with DCO sign-off, e.g.
  `git commit -s -m "perf(image): keep launch image decision local, defer network staleness to background"`.
- This repo mandates push-after-commit (CONTRIBUTING.md). If your dispatch
  context forbids pushing, hold pushes and say so in your report.

## Steps

### Step 1: Reorder `decide_role_image` so local classification runs before any network

In `crates/jackin-runtime/src/runtime/image.rs`, tags-exist branch of
`decide_role_image` (currently lines ~278–425):

1. Delete the `published_image_is_stale` block at lines 278–294 from its
   current position (before recipe computation).
2. Compute `expected_image_recipes` and `inspect_image_labels` exactly as today.
3. In the `classify_image_labels(...)` match:
   - `None` (recipe matches): do **not** run `needs_agent_update` and do **not**
     run any published-image check. Return
     `ImageDecision::RefreshInBackground { image, reason: ImageInvalidationReason::BackgroundCheckPending }`
     **only if** the role declares a published image OR any supported agent has
     a stored version baseline — otherwise return `ImageDecision::Reuse` as
     today. Simpler alternative (preferred if it type-checks cleanly): always
     return `Reuse { image }` here and handle "always spawn the background
     sentinel" in Step 3 instead of encoding it in the decision. Choose the
     simpler alternative unless it breaks an existing test's expectations about
     `RefreshInBackground`.
   - `Some(reason)` (rebuild needed): **now** run the published-image freshness
     check (the code deleted in 1) to decide `BuildFromPublished` vs
     `BuildFromWorkspace`, exactly preserving today's fallback semantics
     (`stale → base_image_override = None`). The tags-empty branch at
     `image.rs:248-276` already does this pattern — mirror it.
4. Keep the tags-empty branch (`image.rs:248-276`) unchanged: when no local
   image exists a build is required anyway and the pull is genuinely needed to
   pick the base.

**Verify**: `cargo check --all-targets` → exit 0.

### Step 2: Add timing spans around the remaining network checks

Wrap the (now build-path-only) published freshness check and the background
agent-version check (Step 3) in
`jackin_diagnostics::active_timing_started("derived image", "published_image_pull", Some(published))`
/ `active_timing_done(...)`, and
`("derived image", "agent_version_check", ...)` respectively — mirroring the
`image_tag_lookup` wrapper at `image.rs:209-223`.

**Verify**: `cargo check --all-targets` → exit 0.

### Step 3: Always spawn the background staleness sentinel on reuse

Today `launch_runtime.rs:1045-1055` spawns `spawn_selected_image_refresh` only
when `ctx.selected_image_refresh` is `Some` (i.e. the foreground already found
a refresh reason). Change the wiring so that on **every** `Reuse` outcome for a
role that (a) declares `published_image`, or (b) has any supported agent with a
stored version baseline (`version_check::stored_version(...).is_some()`), a
background task is spawned after attach handoff that:

1. Runs `needs_agent_update` for all supported agents (network allowed here).
2. Runs the published-image freshness check (pull allowed here).
3. If either says stale → call the existing `prewarm_agent_image` path exactly
   as `spawn_selected_image_refresh` does today (it re-runs `decide_role_image`
   and rebuilds; labels/tags make the result visible to the next launch).

Implementation guidance: extend `SelectedImageRefresh` (defined in
`launch_runtime.rs:69-73`) or add a sibling "sentinel" spawn in
`crate::runtime::image` next to `spawn_selected_image_refresh` — keep the
existing function's contract for the already-known-reason case, add
`spawn_reuse_staleness_sentinel(paths, selector, role_git, branch_override, agent, debug)`
for the new always-on case, and call it from the same place in
`launch_runtime.rs` (after `steps.finish_progress()`, alongside the sibling
prewarm spawns at lines 1056–1071). Emit `run.stage(...)` breadcrumbs mirroring
`spawn_selected_image_refresh`'s (`selected_image_refresh_started/done/failed`)
with names `reuse_staleness_sentinel_*`. Skip spawning in `#[cfg(test)]` builds
exactly as `spawn_selected_image_refresh` does (`image.rs:745-756`).

**Verify**: `cargo check --all-targets` → exit 0.

### Step 4: Preserve diagnostics event semantics

`emit_image_reuse` (`image_decision.rs:193-213`) lists skipped steps in its
detail JSON — add `"published_image_pull"` and `"agent_version_check"` to that
`skipped` array so run diagnostics reflect the new fast path.

**Verify**: `cargo nextest run -p jackin-image` → all pass (update the
`image_decision` tests that assert the detail payload, if any).

### Step 5: Update/extend tests

- In `crates/jackin-runtime/src/runtime/image/tests.rs` (and
  `launch/tests.rs` where decision plumbing is asserted):
  - A test that a recipe-hash match returns `Reuse` **without** any
    `pull_image` call on the mock `DockerApi` (the existing test doubles count
    calls; follow the pattern of tests that assert `list_image_tags` /
    `inspect_image_labels` behavior — grep `pull_image` in the test files for
    the mock seam).
  - A test that a recipe mismatch with a declared published image still
    performs the pull and returns `BuildFromPublished` when fresh /
    `BuildFromWorkspace` when stale (semantics preserved).
- Keep every currently-passing test green; if a test asserts the old
  "pull happens on reuse path" ordering, update it — that ordering is the bug.

**Verify**: `cargo nextest run -p jackin-runtime -p jackin-image` → all pass.

### Step 6: Docs + final gates

Update `docs/content/docs/reference/getting-oriented/architecture.mdx` where it
describes launch-time image freshness (search for "published" in that file):
the published-image check and agent CLI version check now run in the background
after a reuse launch; a stale result rebuilds in the background and the next
launch picks it up.

**Verify**:
- `cargo fmt --check` → exit 0
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0
- `cargo nextest run --all-features` → all pass

## Test plan

- New: reuse-path-no-pull test; build-path-still-pulls test; sentinel-spawn
  gating unit test if the gate predicate is factored into a pure function
  (prefer factoring `fn reuse_needs_background_staleness_check(...) -> bool`
  so it is unit-testable without tokio spawns).
- Model test structure after existing `decide_role_image` tests in
  `crates/jackin-runtime/src/runtime/image/tests.rs`.
- Verification: `cargo nextest run -p jackin-runtime -p jackin-image` all pass,
  including the new tests.

## Done criteria

- [ ] `cargo fmt --check`, clippy gate, `cargo check --all-targets` all exit 0
- [ ] `cargo nextest run --all-features` passes; new tests exist and pass
- [ ] `grep -n "published_image_is_stale" crates/jackin-runtime/src/runtime/image.rs`
      shows no call site that executes before `classify_image_labels` in the
      tags-exist branch
- [ ] `active_timing` spans exist for `published_image_pull` and
      `agent_version_check` (grep confirms)
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The code at the cited lines does not match the excerpts (drift).
- You find a caller that depends on `RefreshInBackground` being returned for
  the published-stale-but-recipe-match case for anything other than spawning
  the background refresh (grep `RefreshInBackground` across `crates/` first —
  known consumers: `launch_core.rs:150-179`, `prewarm_agent_image_from_validated_repo`,
  `prewarm_launch_plan_reason`).
- Test doubles cannot express "pull_image must not be called" without invasive
  mock changes.
- The `restore` path (`restore_pinned_sha`) behaves differently under the new
  ordering — restore replays a pinned recipe and must keep bypassing network
  checks entirely.

## Maintenance notes

- Plan 005 (binary provisioning) assumes the launch path no longer calls
  `latest_release` in the foreground — if 005 lands first, coordinate: its
  cache-first change is still valuable for the build path.
- Reviewers should scrutinize: the `BuildFromPublished` vs `BuildFromWorkspace`
  fallback equivalence, and that the sentinel task cannot pile up (one spawn per
  launch; the repo lock in `prewarm_agent_image` serializes concurrent rebuilds).
- Deferred: coalescing `list_image_tags` + `inspect_image_labels` into one
  Docker call (finding PERF-04 in the audit) — small win, do separately.
