# jackin-usage-snapshot-store

Capsule-local `SQLite` cache for usage quota snapshots:
schema, upserts, and read views. Capsule writes snapshots
after provider refresh; renderers read through the daemon
cache, never by opening this database.

## What this crate owns

- Writes (`write`, `upsert`): snapshot persistence over
  a shared single-thread runtime.
- Reads (`read`, `views`, `buckets`): stored snapshots,
  account views, and bucket presentation.
- Schema (`schema`, `types`): V1 account snapshot
  shape and row types.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | [`tests.rs`](src/tests.rs) |
| `write`/`upsert` | persistence | `tests/` cases |
| `read`/`views`/`buckets` | read views | `tests/` cases |

## Public API

`store_usage_snapshots`, `focused_usage_view`,
`load_account_usage_view`,
`load_all_account_usage_views`,
`list_account_identities`,
`StoredAccountUsageSnapshot`,
`StoredAccountUsageView`,
`AccountIdentitySummary`, `schema_version`
(`store_usage_snapshot` only with
`test-support`).

## How to verify

```sh
cargo nextest run -p jackin-usage-snapshot-store
cargo clippy -p jackin-usage-snapshot-store --all-targets -- -D warnings
```
