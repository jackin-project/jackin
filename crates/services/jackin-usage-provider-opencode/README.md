# jackin-usage-provider-opencode

`OpenCode` Go subscription-limit adapter: API-key loading from
`auth.json`, `zen/go/v1/usage` fetch, and rolling/weekly/monthly
quota buckets. Consumed by `jackin-usage` (host discovery broker,
credential snapshots).

## What this crate owns

- Key loading (`load_opencode_api_key`): the single `opencode-go`
  API credential; secrets never logged.
- Fetch (`fetch_opencode_usage`): `zen/go` usage request with a
  typed key/entitlement/transport/decode taxonomy.
- Parsing (`parse_opencode_usage`): window percents, resets, and
  statuses; over-cap stays raw.
- Errors (`classify_opencode_http_error`, `OpenCodeUsageError`):
  entitlement vs key failures, never conflated.
- Snapshot (`opencode_profile_snapshot`): focused profile view
  with provisional identity.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`adapter.rs`](src/adapter.rs) | adapter surface | [`tests.rs`](src/tests.rs) + [`tests/`](src/tests/) |

## Public API

`opencode_profile_snapshot`, fetch/error types, and key loading
and usage parsing.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-opencode
cargo clippy -p jackin-usage-provider-opencode --all-targets -- -D warnings
```
