# jackin-runtime-launch-slot

Container name
slots + github
preflight.

## What this crate owns

- Slots
  (`launch_slot`):
  `claim_container_name`,
  `claim_known_container_name`,
  `try_acquire_name_lock` —
  flock-backed unique
  name claims.
- Preflight
  (`launch_slot`):
  `verify_github_token_present`,
  `resolve_github_env_map`,
  `github_env_declarations_for_mode` —
  token check + env
  resolution.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`launch_slot.rs`](src/launch_slot.rs) | slots + preflight | hub `launch` suite |

## Public API

`launch_slot::claim_container_name`,
`launch_slot::claim_known_container_name`,
`launch_slot::resolve_github_env_map`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-slot
cargo clippy -p jackin-runtime-launch-slot --all-targets -- -D warnings
```
