# jackin-usage-host-presentation

Plain host surface data types exchanged with native
clients (menu-bar, popover, Usage window): surface
identity, runtime event envelopes, and overview/glance
rows. Every host layer builds on these; the modules
are dependency-free apart from the agent slug mapping.

## What this crate owns

- Surfaces (`surfaces`): the closed `HostSurfaceId`
  domain plus descriptors.
- Events (`events`): `HostUsageEvent` envelopes and
  bounded batches.
- Overview (`overview`): overview and provider
  glance rows.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | (plain types) |
| [`surfaces.rs`](src/surfaces.rs) | surface domain | (plain types) |
| [`events.rs`](src/events.rs) | event envelopes | (plain types) |
| [`overview.rs`](src/overview.rs) | glance rows | (plain types) |

## Public API

`HostSurfaceId`, `HostSurfaceDescriptor`,
`HostUsageEvent`, `HostEventBatch`,
`HostOverviewRow`, `HostProviderGlanceRow`,
`MAX_EVENT_LOG`, `MAX_EVENT_BATCH`.

## How to verify

```sh
cargo nextest run -p jackin-usage-host-presentation
cargo clippy -p jackin-usage-host-presentation --all-targets -- -D warnings
```
