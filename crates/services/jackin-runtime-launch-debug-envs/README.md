# jackin-runtime-launch-debug-envs

Jackin launch
debug runtime
env helper.

## What this crate owns

- Debug envs
  (`debug_envs`):
  `debug_runtime_envs`
  — extra `-e`
  entries the debug
  switch contributes
  to the container
  run args (currently
  none; file-backed
  debug configuration
  must not propagate
  into the container)
  (S7 split 115).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`debug_envs.rs`](src/debug_envs.rs) | debug env entries | hub `launch_runtime` run path + hub launch suite (no dedicated suite) |

## Public API

`debug_envs::debug_runtime_envs`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-debug-envs
cargo clippy -p jackin-runtime-launch-debug-envs --all-targets -- -D warnings
```
