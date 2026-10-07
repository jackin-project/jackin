# jackin-usage-provider-kimi

`Kimi` usage snapshot collection: Code API fetch, rolling/weekly/monthly
pool types, local-server wallet, and quota bucket views. Consumed by
`jackin-usage` (host discovery, credential snapshots).

## What this crate owns

- Response types (`types`): `usages` pools/list shapes, rate limits,
  membership identity.
- API fetch (`fetch`): `coding/v1/usages` fetch + endpoint resolution.
- Local server (`local`): OAuth token loading, Extra Usage wallet buckets.
- Bucket views (`buckets`): pool buckets, over-cap pace labels.
- Snapshot entry (`snapshot`): `kimi_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`types.rs`](src/types.rs) | response + pool types | — |
| [`fetch.rs`](src/fetch.rs) | API fetch + endpoint resolution | — |
| [`local.rs`](src/local.rs) | token loading + wallet buckets | — |
| [`buckets.rs`](src/buckets.rs) | bucket views + pace labels | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`kimi_snapshot`, `fetch_kimi_usage`, pool types, and bucket views consumed
by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-kimi
cargo clippy -p jackin-usage-provider-kimi --all-targets -- -D warnings
```
