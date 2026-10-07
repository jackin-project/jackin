# jackin-usage-host-runtime

Capsule-free host usage runtime
for the macOS menu-bar app and
CLI: presentation state over the
host usage broker.

## What this crate owns

- Runtime (`host`):
  `HostUsageRuntime`, open/config,
  lifecycle, and the broker,
  discovery, and projection glue
  methods.
- Surfaces (`host`): inventory,
  selection, status bar, surface
  control, snapshots, staging,
  desktop, and event log.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`host.rs`](src/host.rs) | runtime struct + API root | `host/tests/` |
| [`host/`](src/host/) | runtime method leaves | `host/tests/` |

## Public API

`host::HostUsageRuntime`,
`host::HostRuntimeConfig`,
`host::HostProbePolicy`, plus the
broker, discovery, credential,
and glance re-exports the host
binary consumes.

## How to verify

```sh
cargo nextest run -p jackin-usage-host-runtime
cargo clippy -p jackin-usage-host-runtime --all-targets -- -D warnings
```
