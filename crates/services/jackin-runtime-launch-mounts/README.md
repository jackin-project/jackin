# jackin-runtime-launch-mounts

Launch bind-mount
assembly per
backend.

## What this crate owns

- Backend (`mounts`):
  `Backend`,
  `resolve_backend` —
  docker vs
  apple-container
  selection.
- Mounts (`mounts`):
  `build_workspace_mounts`,
  `agent_mounts`,
  `apple_agent_mounts`,
  `github_config_mount`.
- Guards (`mounts`):
  provider-authority
  writability checks,
  `AppleContainerMountError`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`mounts.rs`](src/mounts.rs) | assembly | `mounts/tests/` |
| [`mounts/`](src/mounts/) | test suites | `mounts/tests/` |

## Public API

`mounts::build_workspace_mounts`,
`mounts::resolve_backend`,
`mounts::Backend`,
`mounts::agent_mounts`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-mounts
cargo clippy -p jackin-runtime-launch-mounts --all-targets -- -D warnings
```
