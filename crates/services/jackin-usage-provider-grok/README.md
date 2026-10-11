# jackin-usage-provider-grok

`Grok` / `xAI` usage snapshot collection: billing fetch, RPC transport,
and billing cycle views. Consumed by `jackin-usage` (host discovery,
credential snapshots).

## What this crate owns

- Billing types (`types`): billing snapshots, configs, periods.
- Billing fetch (`billing`): REST/RPC billing fetch + tier parsing.
- RPC transport (`rpc`): bearer tokens, RPC requests, protobuf scans.
- Billing views (`views`): cycle labels, bucket views.
- Snapshot entry (`snapshot`): `grok_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`types.rs`](src/types.rs) | billing snapshot types | — |
| [`billing.rs`](src/billing.rs) | billing fetch + tiers | — |
| [`rpc.rs`](src/rpc.rs) | RPC transport + tokens | — |
| [`views.rs`](src/views.rs) | cycle labels + buckets | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`grok_snapshot`, billing fetch, RPC helpers, and views consumed by
`jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-grok
cargo clippy -p jackin-usage-provider-grok --all-targets -- -D warnings
```
