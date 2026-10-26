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

Key snapshots parse the original JSON response text so exact decimal money
values do not pass through floating-point `Value` numbers. The existing
`parse_openrouter_key_usage(Value, ..)` API remains available, but it can use
only the precision retained by its input value. Enable the nondefault
`value-arbitrary-precision` feature to preserve the older exact-number behavior
for that API. Cargo feature unification means opting in also enables serde_json
arbitrary precision for other packages selected in the same dependency graph.
Callers that have original JSON text can use
`parse_openrouter_key_usage_raw` without opting in.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-openrouter
cargo clippy -p jackin-usage-provider-openrouter --all-targets -- -D warnings
```
