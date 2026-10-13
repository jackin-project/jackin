# jackin-usage-provider-muse

`Muse` (Meta) usage observation via MSP `usage/read`: subscription
payload parsing, rolling/weekly window buckets, and cached-observation
views. Over-cap readings stay raw; re-reads never reset freshness.

## What this crate owns

- Parsing (`parse_muse_usage_read`): `observedAtMs`, tier, rolling
  and weekly windows; omitted `usage` is honest absence.
- Identity (`muse_identity_from_value`): `auth.json` email/name;
  secrets stay in the platform store.
- Buckets (`muse_buckets`): window labels with status slots;
  remaining clamps at 0 while used labels carry overage.
- Freshness (`muse_freshness_epoch`): re-reads keep the cached
  stamp unless the observation is new.
- View (`muse_view`): focused usage view over the observation.
- Policy (`MuseKeyExchangePolicy`): the key-exchange endpoint is a
  documented conditional, never a poller.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`adapter.rs`](src/adapter.rs) | observation surface | [`tests.rs`](src/tests.rs) + [`tests/`](src/tests/) |

## Public API

`muse_view`, `parse_muse_usage_read`, `MuseObservation`, and the
key-exchange policy, consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-muse
cargo clippy -p jackin-usage-provider-muse --all-targets -- -D warnings
```
