# jackin-usage-provider-amp

`Amp` usage snapshot collection: CLI/API fetch, subscription parsing, and
quota bucket views. Consumed by `jackin-usage` (host discovery, credential
snapshots).

## What this crate owns

- Response types (`types`): `AmpUsage`, workspace balances, subscriptions.
- CLI/API fetch (`fetch`): usage fetch + API key loading.
- CLI parse (`parse`): `amp usage` output parsing.
- Bucket views (`views`): success views, dollar/orb buckets.
- Snapshot entry (`snapshot`): `amp_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`types.rs`](src/types.rs) | usage + subscription types | — |
| [`fetch.rs`](src/fetch.rs) | CLI/API fetch + key loading | — |
| [`parse.rs`](src/parse.rs) | CLI output parsing | — |
| [`views.rs`](src/views.rs) | success views + buckets | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`amp_snapshot`, `amp_api_key_snapshot`, usage types, and views consumed by
`jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-amp
cargo clippy -p jackin-usage-provider-amp --all-targets -- -D warnings
```
