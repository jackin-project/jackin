# jackin-runtime-usage-relay

Usage relay tunnels
and launch usage
capabilities.

## What this crate owns

- Tunnels
  (`usage_relay`):
  `prepare_for_stdio_tunnel`,
  `start_docker_tunnel`,
  `start_apple_tunnel`,
  `UsageRelayGuard` —
  host usage-broker
  traffic into role
  containers.
- Capabilities
  (`usage_relay`):
  `populate_launch_usage_capabilities`,
  `forwarded_sources_from_launch_config`,
  `resolved_launch_usage_inventory` —
  launch usage
  inventory.
- Mounts
  (`usage_relay`):
  `docker_runtime_mount`,
  `apple_runtime_mount` —
  runtime socket
  mounts.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`usage_relay.rs`](src/usage_relay.rs) | tunnels + capabilities | `usage_relay/tests.rs` |

## Public API

`usage_relay::prepare_for_stdio_tunnel`,
`usage_relay::start_docker_tunnel`,
`usage_relay::start_apple_tunnel`,
`usage_relay::populate_launch_usage_capabilities`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-usage-relay
cargo clippy -p jackin-runtime-usage-relay --all-targets -- -D warnings
```
