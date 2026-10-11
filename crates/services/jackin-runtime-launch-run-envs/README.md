# jackin-runtime-launch-run-envs

Jackin launch
run invocation
env helper.

## What this crate owns

- Run envs
  (`run_envs`):
  `run_runtime_envs`
  — extra `-e`
  entries identifying
  the current run
  (`JACKIN_INVOCATION_ID`
  for the active
  telemetry invocation,
  none when no
  invocation is active)
  (S7 split 116).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`run_envs.rs`](src/run_envs.rs) | run env entries | hub `launch_runtime` run path (no dedicated suite) |

## Public API

`run_envs::run_runtime_envs`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-run-envs
cargo clippy -p jackin-runtime-launch-run-envs --all-targets -- -D warnings
```
