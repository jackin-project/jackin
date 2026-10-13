# jackin-usage-provider-minimax

`MiniMax` usage snapshot collection: Token Plan remains, PAYG balances,
region routing, and quota bucket views. Consumed by `jackin-usage` (host
discovery, credential snapshots).

## What this crate owns

- Response types (`types`): remains responses, combo cards, model windows.
- Remains fetch (`fetch`): remains/balance fetch + endpoint resolution.
- PAYG balances (`balance`): balance decode + buckets.
- Region routing (`region`): key-product + region resolution.
- Bucket views (`buckets`): window buckets, boost notes, count lines.
- Snapshot entry (`snapshot`): `minimax_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`types.rs`](src/types.rs) | response + window types | — |
| [`fetch.rs`](src/fetch.rs) | remains fetch + endpoints | — |
| [`balance.rs`](src/balance.rs) | PAYG balance buckets | — |
| [`region.rs`](src/region.rs) | key-product + region routing | — |
| [`buckets.rs`](src/buckets.rs) | bucket views + notes | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`minimax_snapshot`, `fetch_minimax_usage`, remains types, and bucket views
consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-minimax
cargo clippy -p jackin-usage-provider-minimax --all-targets -- -D warnings
```
