# jackin-runtime-launch-failure

Launch failure
title, diagnosis,
and CLI error
rendering, plus
role-source
resolution.

## What this crate owns

- Rendering
  (`failure`):
  `launch_failure_title`,
  `short_launch_diagnosis`,
  `launch_failure_cli_error` —
  stage-specific
  titles and the
  pass-through CLI
  error for failed
  launches.
- Resolution
  (`failure`):
  `resolve_launch_role_source` —
  manifest role
  source for the
  failure path,
  honoring the
  recorded restore
  override.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`failure.rs`](src/failure.rs) | failure rendering + role-source resolve | hub `launch` suite (`case_01`, `case_08`) |

## Public API

`failure::launch_failure_title`,
`failure::short_launch_diagnosis`,
`failure::launch_failure_cli_error`,
`failure::resolve_launch_role_source`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-failure
cargo clippy -p jackin-runtime-launch-failure --all-targets -- -D warnings
```
