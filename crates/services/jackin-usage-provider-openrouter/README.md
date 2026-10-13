# jackin-usage-provider-openrouter

`OpenRouter` key/account usage snapshot collection: key quota, credit
balance, and model catalog checks. Consumed by `jackin-usage` (host
discovery, credential snapshots).

## What this crate owns

- Response types (`types`): key data, credits outcomes, model checks.
- Key/credit fetch (`fetch`): `/key` + `/credits` fetch, base URLs.
- Response parse (`parse`): quota decode, credits buckets.
- Snapshot entry (`snapshot`): `openrouter_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`types.rs`](src/types.rs) | key/credit/model types | — |
| [`fetch.rs`](src/fetch.rs) | key/credit fetch + base URLs | — |
| [`parse.rs`](src/parse.rs) | quota decode + credits buckets | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`openrouter_snapshot`, key/credit fetch, and quota types consumed by
`jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-openrouter
cargo clippy -p jackin-usage-provider-openrouter --all-targets -- -D warnings
```
