# Plan 025: Burn down the 66 `too_many_lines` / `cognitive_complexity` suppressions

> **Executor instructions**: Incremental refactor — one suppression site at a time, behavior-preserving.
> Run tests after each extraction. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin/src/console/tui/run.rs crates/jackin-capsule/src/daemon/input_dispatch.rs crates/jackin-runtime/src/runtime/launch`

## Status

- **Priority**: P3
- **Effort**: M (incremental; can be done in slices)
- **Risk**: LOW-MED
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

There are **66** non-test `too_many_lines`/`cognitive_complexity` allow/expect sites across the workspace —
each a deliberate override of a health lint (`clippy.toml`/CI run these as `warn` under `-D warnings`) on a
specific oversized function. They cluster on the highest-churn control-flow hubs: the console run loop
(`run.rs`, 4 sites), the launch pipeline (`launch_runtime.rs`, `launch_core.rs`, `host_attach.rs`, 3 each),
and the daemon input dispatch (`daemon.rs`, `input_dispatch.rs`, 3 each). Concentrating complexity where
bugs are costliest. Each suppression is a self-declared hotspot to extract.

## Current state

- 66 suppressions total (non-test): confirm with
  `grep -rn "too_many_lines\|cognitive_complexity" crates --glob '!**/tests.rs' | grep -c "expect\|allow"`.
- Densest: `crates/jackin/src/console/tui/run.rs` (4); `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs`,
  `.../launch_pipeline/launch_core.rs`, `crates/jackin-runtime/src/runtime/host_attach.rs`,
  `crates/jackin-capsule/src/daemon.rs`, `crates/jackin-capsule/src/daemon/input_dispatch.rs` (3 each).

## Scope

**In scope:** the suppression sites, refactored by extracting helpers. Start with `run.rs` and
`input_dispatch.rs` (highest churn). **Out of scope:** behavior changes; the lint config itself (don't
raise/lower the lint — fix the functions).

## Steps

Do this as a **slice per PR**, not all 66 at once.

### Step 1: Pick a file and extract its oversized functions

For each `#[expect(clippy::too_many_lines …)]` / `cognitive_complexity` function in the chosen file
(start `run.rs`), extract cohesive sub-blocks (match arms, phases, validation groups) into named private
helpers so the function drops below the lint threshold, then **delete the `#[expect]`**. The extraction
must be behavior-preserving — same control flow, just named.

**Verify per function**: `cargo clippy -p <crate> --all-targets -- -D warnings` → exit 0 with the `#[expect]`
removed (if it still needs the suppression, the extraction wasn't enough — keep going or STOP);
`cargo nextest run -p <crate>` → all pass.

### Step 2: Repeat for the next densest file

`input_dispatch.rs`, then the launch pipeline files, one file per slice. Record progress (sites remaining)
in this plan's row note after each slice.

## Done criteria (per slice; the plan is "done" when the count reaches an agreed floor)

- [ ] Chosen file's `too_many_lines`/`cognitive_complexity` `#[expect]`s removed via extraction (or a note
      why a specific one is irreducible)
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` still passes
- [ ] `cargo nextest run -p <crate>` green (behavior preserved)
- [ ] Suppression count recorded (start 66 → current N) in the row note
- [ ] `plans/README.md` row updated

## STOP conditions

- A function is genuinely irreducible without a real design change (e.g. a giant exhaustive `match` that's
  clearer whole) — leave the `#[expect]` with a **reason** naming why, and note it; don't force an
  extraction that hurts readability (that violates the repo's own comment/DRY spirit).
- An extraction changes behavior (a test flips) — revert that extraction; the function had hidden coupling.

## Maintenance notes

- This is a "leave it better than you found it" backlog — a reviewer touching any of these files should
  extract its function rather than re-adding an `#[expect]`.
- The goal isn't zero suppressions; it's that each surviving one is genuinely justified (with a `reason`),
  not inertia.
