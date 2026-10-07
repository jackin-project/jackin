# jackin-usage-provider-zai

`Z.AI` / `GLM` usage snapshot collection: quota fetch, CN team scope, and
quota bucket views. Consumed by `jackin-usage` (host discovery, credential
snapshots).

## What this crate owns

- Quota fetch (`fetch`): endpoint resolution, team-scope headers, `success:
  false` no-plan handling.
- Quota types (`quota`): `ZAI` response decoding, plan windows, semantic
  status slots.
- Bucket views (`buckets`): peak-rate notes, model breakdowns, count lines.
- Snapshot entry (`snapshot`): `provider_key_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`fetch.rs`](src/fetch.rs) | quota fetch + endpoint resolution | — |
| [`quota.rs`](src/quota.rs) | response types + plan windows | — |
| [`buckets.rs`](src/buckets.rs) | bucket views + rate notes | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`provider_key_snapshot`, `fetch_zai_usage`, quota types, and bucket views
consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-zai
cargo clippy -p jackin-usage-provider-zai --all-targets -- -D warnings
```
