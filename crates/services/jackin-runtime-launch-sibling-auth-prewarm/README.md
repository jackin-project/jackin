# jackin-runtime-launch-sibling-auth-prewarm

Sibling-agent auth prewarm
spawn/await for a launch
role.

## What this crate owns

- Prewarm
  (`sibling_auth_prewarm`):
  `spawn_sibling_auth_prewarm` /
  `await_sibling_auth_prewarm` —
  blocking-pool auth-slot
  prewarm for a role's
  non-selected agents plus
  the admission-gate wait.
  The runtime core keeps
  the real launch (S7
  split 97).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`sibling_auth_prewarm.rs`](src/sibling_auth_prewarm.rs) | spawn/await prewarm | hub `launch_runtime` suite (`launch_runtime/tests/case_01.rs`, `case_02.rs`) |

## Public API

`sibling_auth_prewarm::SiblingAuthPrewarm`,
`sibling_auth_prewarm::spawn_sibling_auth_prewarm`,
`sibling_auth_prewarm::await_sibling_auth_prewarm`,
`sibling_auth_prewarm::spawn_auth_prewarm_worker`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-sibling-auth-prewarm
cargo clippy -p jackin-runtime-launch-sibling-auth-prewarm --all-targets -- -D warnings
```
