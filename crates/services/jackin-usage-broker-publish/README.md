# jackin-usage-broker-publish

Canonical broker projection
publication: account, window, and
group builders plus the incremental
per-account publisher.

## What this crate owns

- Projection builders
  (`account_windows`, `freshness`,
  `groups`, `windows`): pure views
  from snapshots and buckets.
- Publication (`errors`, `merge`,
  `publisher`, `quota`, `revoke`):
  per-account merge, durable
  checkpointing, revocation, and
  quota states.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`publisher.rs`](src/publisher.rs) | merge + checkpoint | [`tests/`](src/tests/) |
| [`windows.rs`](src/windows.rs) | view accounts | (via `tests`) |
| [`groups.rs`](src/groups.rs) | metric groups | (via `tests`) |
| [`freshness.rs`](src/freshness.rs) | freshness/quota | (via `tests`) |
| [`account_windows.rs`](src/account_windows.rs) | limit windows | (via `tests`) |
| [`merge.rs`](src/merge.rs) | view merge | (via `tests`) |
| [`revoke.rs`](src/revoke.rs) | revocation | (via `tests`) |
| [`quota.rs`](src/quota.rs) | bucket states | (via `tests`) |
| [`errors.rs`](src/errors.rs) | error mapping | (via `tests`) |

## Public API

`ProjectionPublisher`,
`AccountIdentityMetadata`,
`merge_views`, `account_for_view`,
`project_window`, `project_groups`,
`metric_groups_for_view`,
`lifecycle`, `failure_lifecycle`,
freshness/quota helpers, and the
publication error constructors.

## How to verify

```sh
cargo nextest run -p jackin-usage-broker-publish
cargo clippy -p jackin-usage-broker-publish --all-targets -- -D warnings
```
