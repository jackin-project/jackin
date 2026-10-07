# jackin-runtime-attach-sessions

Agent session
inventory inspection.

## What this crate owns

- Inventory
  (`sessions`):
  `AgentSession`,
  `AgentSessionInventory`,
  `inspect_agent_sessions`,
  `parse_jackin_sessions` —
  capsule status
  query + parse.
- Messages
  (`sessions`):
  `docker_unavailable_msg`,
  `inspect_unavailable_message` —
  shared inspect-failure
  text.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`sessions.rs`](src/sessions.rs) | inventory + messages | hub `attach` suite |

## Public API

`sessions::AgentSession`,
`sessions::AgentSessionInventory`,
`sessions::inspect_agent_sessions`,
`sessions::parse_jackin_sessions`,
`sessions::docker_unavailable_msg`,
`sessions::inspect_unavailable_message`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-sessions
cargo clippy -p jackin-runtime-attach-sessions --all-targets -- -D warnings
```
