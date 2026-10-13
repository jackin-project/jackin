# jackin-runtime-attach-inspect

Instance
inspection.

## What this crate owns

- Inspection
  (`inspect`):
  `inspect_hardline_instance`,
  `inspect_docker_network`,
  `missing_restore_message` —
  recovery render +
  restore hints.
- Descriptions
  (`inspect`):
  `describe_agent_session_count`,
  `describe_agent_sessions`,
  `describe_network_state`,
  `describe_mount_state` —
  state labels.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`inspect.rs`](src/inspect.rs) | inspection | hub `attach` suite |

## Public API

`inspect::inspect_hardline_instance`,
`inspect::describe_agent_session_count`,
`inspect::missing_restore_message`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-inspect
cargo clippy -p jackin-runtime-attach-inspect --all-targets -- -D warnings
```
