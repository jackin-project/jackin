# jackin-usage-destination

Typed interactive destinations
outside the canonical JSON
projection, reconciled against one
immutable publication.

## What this crate owns

- Destinations (`destination`):
  `UsageDestination` selection,
  `NormalizedUsageDestination`
  reconciliation, and the
  `ProjectionMetadata` view.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`destination.rs`](src/destination.rs) | selection | (via `jackin-usage`) |

## Public API

`UsageDestination`,
`NormalizedUsageDestination`,
`ProjectionMetadata`, and
`normalize_destination`.

## How to verify

```sh
cargo nextest run -p jackin-usage-destination
cargo clippy -p jackin-usage-destination --all-targets -- -D warnings
```
