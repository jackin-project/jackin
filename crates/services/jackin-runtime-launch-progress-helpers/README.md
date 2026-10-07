# jackin-runtime-launch-progress-helpers

Launch progress
step helpers.

## What this crate owns

- Steps
  (`progress_helpers`):
  `StepCounter`,
  stage start/done/skip/fail —
  step boundaries
  with stage telemetry.
- Prompt
  (`progress_helpers`):
  `LaunchEnvPrompter`,
  `sensitive_mount_prompt` —
  rich-dialog prompt
  bridge.
- Labels
  (`progress_helpers`):
  `launch_target_kind`,
  `launch_target_label`,
  `launch_mount_lines` —
  summary rendering.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`progress_helpers.rs`](src/progress_helpers.rs) | steps + prompts + labels | [`tests.rs`](src/progress_helpers/tests.rs) (1 case) |

## Public API

`progress_helpers::StepCounter`,
`progress_helpers::LaunchEnvPrompter`,
`progress_helpers::sensitive_mount_prompt`,
`progress_helpers::launch_target_kind`,
`progress_helpers::launch_target_label`,
`progress_helpers::launch_mount_lines`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-progress-helpers
cargo clippy -p jackin-runtime-launch-progress-helpers --all-targets -- -D warnings
```
