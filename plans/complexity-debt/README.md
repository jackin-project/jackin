# Complexity-debt burndown

The complexity-suppression burndown opened as plan 025 in the 2026-07-03 deep audit (PR #713). The
first slice (`crates/jackin/src/console/tui/run.rs`) shipped in that PR; the remaining suppression sites
stay here until they hit an agreed floor.

## Current state

- 25 non-test `too_many_lines` / `cognitive_complexity` suppression sites remain workspace-wide (down
  from 66 at audit time).
- 8 production whole-file `#![allow(clippy::too_many_lines)]` directives carry no `reason=`, violating
  the plan's maintenance goal that each surviving suppression be justified rather than left as inertia:
  - `crates/jackin-capsule/src/daemon/input_dispatch.rs:1`
  - `crates/jackin-protocol/src/attach.rs:1`
  - `crates/jackin-launch-tui/src/tui/subscriptions.rs:1`
  - `crates/jackin-launch-tui/src/tui/run.rs:1`
  - `crates/jackin-console/src/tui/input/global_mounts.rs:1`
  - `crates/jackin-console/src/tui/input/global_mounts/auth.rs:1`
  - `crates/jackin-runtime/src/runtime/repo_cache.rs:1`
  - `crates/jackin-runtime/src/runtime/launch/restore_resolve.rs:1`

## What remains

Per `025-complexity-suppression-burndown.md`:

- Agree on a floor count (the plan's done-criteria requires one; none agreed yet).
- For each remaining site: either extract helpers so the function drops below the lint threshold and
  delete the suppression, or attach a `reason=` naming why it is irreducible.
- Priority order from the plan: `input_dispatch.rs` and the launch pipeline files next (highest churn
  after `run.rs`).

Verify per slice:

```sh
cargo clippy -p <crate> --all-targets -- -D warnings
cargo nextest run -p <crate>
```
