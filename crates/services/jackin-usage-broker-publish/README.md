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
| [`publisher.rs`](src/publisher.rs) | merge + checkpoint | — |
| [`windows.rs`](src/windows.rs) | view accounts | — |
| [`groups.rs`](src/groups.rs) | metric groups | — |
| [`freshness.rs`](src/freshness.rs) | freshness/quota | — |
| [`account_windows.rs`](src/account_windows.rs) | limit windows | — |
| [`merge.rs`](src/merge.rs) | view merge | — |
| [`revoke.rs`](src/revoke.rs) | revocation | — |
| [`quota.rs`](src/quota.rs) | bucket states | — |
| [`errors.rs`](src/errors.rs) | error mapping | — |

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
cargo nextest run -p jackin-usage-broker-publish --no-tests warn
cargo clippy -p jackin-usage-broker-publish --all-targets -- -D warnings
```
