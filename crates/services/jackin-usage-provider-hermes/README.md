# jackin-usage-provider-hermes

`Hermes` TUI attribution adapter: Nous Portal subscription parsing and
attributed usage views. No Hermes-native quota API exists — usage is
attributed to underlying provider accounts or the Portal subscription.

## What this crate owns

- Runtime (`HermesRuntime`): exclusively owned profiles; clones drop
  rotating OAuth grants.
- Portal subscription (`parse_hermes_subscription`): tier, credit
  strings, and cycle end; decimal strings pass through verbatim.
- Views (`hermes_view`, `hermes_subscription_bucket`): attributed
  buckets with `renews <date>` pace notes, never reset stamps.
- Tracker counters (`hermes_tracker_counter_buckets`): display
  state, always zero buckets.
- Auth status (`hermes_auth_status`): Portal failures fail closed.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`adapter.rs`](src/adapter.rs) | adapter surface | [`tests.rs`](src/tests.rs) |

## Public API

`hermes_view`, `parse_hermes_subscription`, `HermesRuntime`, and
`HermesSubscription`, consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-hermes
cargo clippy -p jackin-usage-provider-hermes --all-targets -- -D warnings
```
