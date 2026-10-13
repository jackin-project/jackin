# jackin-usage-provider-antigravity

`Antigravity` (`agy`) usage snapshot collection: CLI fetch, quota parsing,
and bucket views. Consumed by `jackin-usage` (host discovery, credential
snapshots).

## What this crate owns

- Quota types (`types`): families, pools, windows.
- CLI fetch (`cli`): `agy` usage/credits fetch + version gate.
- Quota parse (`parse`): summary/legacy response parsing.
- Credit buckets (`credits`): credit balance buckets.
- Bucket views (`buckets`): family buckets, identity, plans.
- Snapshot entry (`snapshot`): `antigravity_snapshot` for CLI surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`types.rs`](src/types.rs) | family/pool/window types | — |
| [`cli.rs`](src/cli.rs) | CLI fetch + version gate | — |
| [`parse.rs`](src/parse.rs) | quota response parsing | — |
| [`credits.rs`](src/credits.rs) | credit balance buckets | — |
| [`buckets.rs`](src/buckets.rs) | buckets + identity + plans | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`antigravity_snapshot`, CLI fetch, quota types, and buckets consumed by
`jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-antigravity
cargo clippy -p jackin-usage-provider-antigravity --all-targets -- -D warnings
```
