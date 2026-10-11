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

## Capability ownership

Before a tunnel starts, the relay attaches to the host broker and asks it to
resolve the launch's forwarded sources against the broker-owned catalog. The
request contains selected account ids and surfaces, forwarded profile and
environment-key metadata, and exact fingerprints for staged credentials. It
contains no credential values. The broker returns only the capabilities whose
source requirements match those facts; the relay pins that allowlist for the
container lifetime and keeps the staged credential scope for each forwarded
refresh.

Credential declarations that require operator interaction remain unavailable
to unattended broker discovery. In particular, an `OpRef` source does not
grant a relay capability unless the broker has independently obtained the
matching material through a supported non-interactive source. The broker
projection reports the typed interaction-required issue; the relay remains
fail-closed until the credential becomes available.
The current explicit auth-preparation flow covers Claude Keychain credentials
only and does not prepare `OpRef`-backed environment sources.

## How to verify

```sh
cargo nextest run -p jackin-runtime-usage-relay
cargo clippy -p jackin-runtime-usage-relay --all-targets -- -D warnings
```
