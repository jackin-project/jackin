# Plan 006: Stop `published_image_stale` from nuking the Docker layer cache; fix the cache-bust ping-pong

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-runtime/src/runtime/image.rs crates/jackin-image/src/version_check.rs`
> On mismatch with the excerpts below, STOP. Coordinate with Plans 001/003 if
> they landed first (same file, different regions).

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none (shares `runtime/image.rs` with 001 — rebase carefully)
- **Category**: perf + bug
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

When the image decision reason is `PublishedImageStale` — the routine outcome
whenever the role repo has a commit its CI hasn't published yet —
`build_agent_image` escalates to full rebuild semantics: it forces
`ensure_local_role_base` to rebuild the role base with `--pull` and mints a
fresh `JACKIN_CACHE_BUST` timestamp, deliberately busting Docker layer cache
that is still perfectly valid (nothing about a role commit invalidates the
construct base layers or the fallback-installer layers). A measured run paid
**208 s in `build_role_base` + 50 s in `docker_build`** for exactly this class.
Only the operator's explicit `--rebuild` should carry bust-the-world semantics.

Separately (correctness): a background refresh triggered by
`AgentVersionChanged` replays the *stored* cache-bust, so for agents installed
via the script-fallback path (Claude/Grok when prefetch failed) the network
installer layer cache-hits, the old version is re-recorded, and the refresh
retriggers every launch forever — a rebuild loop that never converges.

## Current state

All in `crates/jackin-runtime/src/runtime/image.rs` unless noted.

The escalation (build_agent_image):

```rust
// image.rs:1405
let rebuild = rebuild || build_reason == ImageInvalidationReason::PublishedImageStale;
```

flows into `ensure_local_role_base(…, rebuild, …)` which, when `rebuild`:
skips the reuse check (lines 1201–1220) and adds `--pull`:

```rust
// image.rs:1277-1280
let construct_is_locally_built = construct != jackin_manifest::repo_contract::CONSTRUCT_IMAGE;
if rebuild && !construct_is_locally_built {
    args.push("--pull");
}
```

and into the cache-bust mint (lines 1532–1548):

```rust
let cache_bust_value = if !supported_set_uses_cache_bust(&validated_repo.manifest) {
    "unused".to_owned()
} else if rebuild {
    // …fresh timestamp; store_cache_bust(paths, &image, &ts)
} else {
    version_check::stored_cache_bust(paths, &image).unwrap_or_else(|| "0".to_owned())
};
```

Context on intent (from code comments): the fresh-timestamp arm exists to
invalidate *fallback installer* layers on an operator `--rebuild`
(lines 1503–1516); `supported_set_uses_cache_bust` is true only when
Claude/Grok are in the supported set (`image_recipe.rs:180-185`), because only
their fallback installers are non-reproducible network steps.

The ping-pong (background refresh path): `RefreshInBackground { reason:
AgentVersionChanged }` → `prewarm_agent_image_from_validated_repo` →
`build_agent_image(…, rebuild=false, reason, …)` (lines 920–947) → stored bust
replayed → fallback installer layer cache-hits → same old agent version →
`record_built_agent_version` re-stores it → next launch: stale again.
(`version_check.rs:68-86` compares stored vs latest.)

`ImageInvalidationReason` variants: `image_decision.rs:12-27`.

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-runtime -p jackin-image`                      | all pass            |
| Full      | `cargo nextest run --all-features`                                         | all pass            |

## Scope

**In scope**:
- `crates/jackin-runtime/src/runtime/image.rs` (build_agent_image,
  ensure_local_role_base call plumbing) + `runtime/image/tests.rs`
- `crates/jackin-image/src/image_decision.rs` only if reason plumbing needs a
  helper predicate (+tests)
- `docs/content/docs/developing/construct-image.mdx` if it documents rebuild
  semantics (grep "rebuild"; update the `--rebuild` description if present)

**Out of scope**:
- decide-path restructure (Plan 001), layer order (002), binaries (005).
- Changing what `--rebuild` (ExplicitRebuild) does — it keeps full-bust
  semantics.

## Git workflow

- Branch: `git checkout -b fix/rebuild-cache-preservation` off `main`.
- `git commit -s -m "perf(image): preserve layer cache on published-stale rebuilds"` and
  `git commit -s -m "fix(image): mint cache bust on agent-version refresh to break rebuild loop"`.
- Push after commit per CONTRIBUTING.md unless dispatch forbids it.

## Steps

### Step 1: Split "reason forces a build" from "reason forces cache busting"

In `build_agent_image`:

1. Delete the escalation at line 1405.
2. Introduce two locals derived from inputs:
   - `let force_base_rebuild = rebuild;` — only the operator's explicit
     rebuild skips base reuse and `--pull`s.
   - `let mint_fresh_cache_bust = rebuild || build_reason == ImageInvalidationReason::AgentVersionChanged;`
     — explicit rebuild AND version-driven refreshes bust the installer layers
     (the latter fixes the ping-pong).
3. Pass `force_base_rebuild` where `rebuild` currently flows into
   `ensure_local_role_base` (line 1413-1426) and into the construct-mismatch
   short-circuit (line 1521: `if rebuild { false } else { … }` — keep keyed on
   `force_base_rebuild`).
4. Key the cache-bust mint (line 1532–1548) on `mint_fresh_cache_bust`.
5. `PublishedImageStale` now behaves like any other build reason: base reuse
   check runs (the base for the *new* role SHA won't exist, so the base
   Dockerfile builds — but **with** layer cache and **without** `--pull`),
   and the stored cache-bust replays.

**Verify**: `cargo check --all-targets` → exit 0.

### Step 2: Confirm construct freshness is not lost

The `--pull` on rebuild existed to refresh the construct base. Verify the
non-rebuild path still detects construct changes: the recipe includes
`construct_image` (name) and `classify_image_labels` flags
`ConstructImageChanged` (`image_decision.rs:163-166`) — name changes rebuild.
A same-tag construct *digest* drift (upstream re-published `construct:latest`)
is only picked up by `--pull`; that refresh now happens **only** on operator
`--rebuild`. Confirm this is the documented contract: check
`docs/content/docs/developing/construct-image.mdx` for how construct updates
roll out (versioned tags vs mutable latest). If construct is consumed via a
**versioned tag** (expected: `jackin_manifest::repo_contract::CONSTRUCT_IMAGE`
pins a tag — read the constant), digest drift is not a supported flow and no
action is needed; note the finding in the PR body. If it is a mutable tag,
STOP and report (the fix then needs a digest check, not `--pull`).

**Verify**: statement in PR body + the constant's value quoted.

### Step 3: Tests

In `runtime/image/tests.rs` (existing build_agent_image tests use the
CommandRunner seam that records docker argv):

- `PublishedImageStale` build: recorded `buildx build` argv for the role base
  contains **no** `--pull`; cache-bust arg equals the stored value.
- `ExplicitRebuild`: `--pull` present (default construct), fresh bust minted +
  stored (existing tests likely cover; keep green).
- `AgentVersionChanged` refresh: fresh bust minted and `store_cache_bust`
  called (fixes ping-pong; assert stored value changed).
- Construct-mismatch short-circuit still skipped when operator-rebuild.

**Verify**: `cargo nextest run -p jackin-runtime -p jackin-image` → all pass.

### Step 4: Gates + docs

Update the `--rebuild` / rebuild-semantics paragraph in
`construct-image.mdx` if present (Step 2 grep). Run full gates.

**Verify**: fmt + clippy + `cargo nextest run --all-features` green.

## Test plan

Covered in Step 3 — model after existing argv-recording tests in
`runtime/image/tests.rs` (grep `"--pull"` there to find the seam).

## Done criteria

- [ ] `grep -n "PublishedImageStale" crates/jackin-runtime/src/runtime/image.rs`
      shows no line OR-ing it into `rebuild`
- [ ] Argv tests: no `--pull` on published-stale builds; fresh bust on
      agent-version refresh (both asserted)
- [ ] fmt/clippy/check/`cargo nextest run --all-features` green
- [ ] No files outside in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- Step 2 reveals a mutable construct tag as the supported update channel.
- The cache-bust value participates in the recipe hash in a way that makes a
  minted bust force a *decide-path* mismatch loop (read
  `cache_bust_recipe_value`, `image_recipe.rs:187-197`: expected recipes use the
  *stored* bust, and the mint path stores the new value before labeling —
  verify the stored value and the label agree after a refresh; if they cannot,
  report the sequencing instead of patching).
- Plans 001/003 landed and moved the cited regions beyond recognition.

## Maintenance notes

- Worst-case launch cost for "role commit + CI lag" drops from ~4 min
  (uncached base + overlay) to a normal cached build. Watch the first
  post-merge published-stale build in run diagnostics (`build_role_base`
  should be seconds when only role layers changed).
- Reviewer focus: exact equivalence of `ExplicitRebuild` behavior before/after.
- Related deferred item (README): decide-path pull deferral is Plan 001;
  role-base context copy → hardlink/tar streaming (audit PERF-06).
