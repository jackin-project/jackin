# jackin-runtime-launch-plan

Launch plan vocabulary
and diagnostics
emission.

## What this crate owns

- Plan (`launch_plan`):
  `LaunchPlan` — attach,
  start, create, build,
  prewarm variants.
- Emit (`launch_plan`):
  `emit_launch_plan`,
  `emit_prewarm_launch_plan`,
  `emit_image_materialization_plan`,
  `emit_rejected_launch_plan_for_run`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`launch_plan.rs`](src/launch_plan.rs) | plan + emits | — |

## Public API

`launch_plan::LaunchPlan`,
`launch_plan::emit_launch_plan`,
`launch_plan::emit_prewarm_launch_plan`,
`launch_plan::emit_image_materialization_plan`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-plan
cargo clippy -p jackin-runtime-launch-plan --all-targets -- -D warnings
```
