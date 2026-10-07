# jackin-runtime-discovery

Role container discovery
via Docker label queries.

## What this crate owns

- List (`discovery`):
  `list_role_names`,
  `list_running_agent_names`,
  `list_managed_role_names`.
- Display (`discovery`):
  `list_running_agent_display_names`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`discovery.rs`](src/discovery.rs) | label queries | `discovery/tests.rs` |
| [`discovery/`](src/discovery/) | test suites | `discovery/tests.rs` |

## Public API

`discovery::list_role_names`,
`discovery::list_running_agent_names`,
`discovery::list_managed_role_names`,
`discovery::list_running_agent_display_names`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-discovery
cargo clippy -p jackin-runtime-discovery --all-targets -- -D warnings
```
