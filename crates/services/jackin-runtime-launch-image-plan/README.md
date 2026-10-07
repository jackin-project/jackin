# jackin-runtime-launch-image-plan

Launch image
plan: resolve
the role repo,
decide the image.

## What this crate owns

- Plan
  (`image_plan`):
  `LaunchImagePlan`,
  `resolve_launch_image_plan`,
  `plan_from_decision` —
  image decision.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`image_plan.rs`](src/image_plan.rs) | plan | `image_plan/tests.rs` |

## Public API

`image_plan::LaunchImagePlan`,
`image_plan::resolve_launch_image_plan`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-image-plan
cargo clippy -p jackin-runtime-launch-image-plan --all-targets -- -D warnings
```
