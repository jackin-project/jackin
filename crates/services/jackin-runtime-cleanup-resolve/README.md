# jackin-runtime-cleanup-resolve

Cleanup handle
resolution.

## What this crate owns

- Resolution
  (`resolve`):
  `resolve_cleanup_handles_for_state`,
  `resolve_role_handle_for_state`,
  `resolve_dind_handle_for_state`,
  `docker_resources_for_state` —
  ownership-checked
  handles from recorded
  state.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`resolve.rs`](src/resolve.rs) | resolution | hub `cleanup` suite |

## Public API

`resolve::resolve_cleanup_handles_for_state`,
`resolve::resolve_role_handle_for_state`,
`resolve::docker_resources_for_state`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-resolve
cargo clippy -p jackin-runtime-cleanup-resolve --all-targets -- -D warnings
```
