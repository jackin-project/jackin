# jackin-usage-provider-cursor

`Cursor` usage snapshot collection: personal allowance vs Enterprise Admin
reporting. Consumed by `jackin-usage` (host discovery, credential
snapshots).

## What this crate owns

- Auth + identity (`auth`): selected-account credentials, CLI identity.
- Period usage (`period`): current-period dashboard usage.
- Plan + grants (`plan`): plan info, credit grants.
- Request usage (`requests`): request counts + buckets.
- Session REST (`rest`): `cursor.com` session REST helpers.
- Sand usage (`sand`): sandbox usage status.
- Usage summary (`summary`): usage summaries, Stripe balances.
- Teams (`teams`): Enterprise spend + usage events.
- Snapshot entry (`snapshot`): `cursor_snapshot` for keyed surfaces.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`auth.rs`](src/auth.rs) | auth + identity + endpoints | — |
| [`period.rs`](src/period.rs) | period usage | — |
| [`plan.rs`](src/plan.rs) | plan info + grants | — |
| [`requests.rs`](src/requests.rs) | request usage | — |
| [`rest.rs`](src/rest.rs) | session REST | — |
| [`sand.rs`](src/sand.rs) | sandbox usage | — |
| [`summary.rs`](src/summary.rs) | summaries + balances | — |
| [`teams.rs`](src/teams.rs) | Enterprise spend/events | — |
| [`snapshot.rs`](src/snapshot.rs) | snapshot entry point | — |

## Public API

`cursor_snapshot`, `cursor_profile_snapshot`, usage fetch, and buckets
consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-cursor
cargo clippy -p jackin-usage-provider-cursor --all-targets -- -D warnings
```
