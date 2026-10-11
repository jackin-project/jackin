# jackin-usage-host-accounts

Canonical, multi-source account inventory for the host
runtime: catalog materialization over live, durable,
and discovered sources, plus identity evidence and
selected-account persistence.

## What this crate owns

- Catalog (`catalog`): account records and
  selected-account persistence.
- Identity (`identity`): canonical identity
  evidence and the alias graph.
- Materialization (`materialize`): the catalog
  build plus the `AccountCatalogStores` /
  `AccountMembershipDescriptor` seams.
- Views (`views`): account keys and labels.
- Lifecycle (`lifecycle`): lifecycle and
  provenance.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | (none yet) |
| `catalog`/`identity` | records + identity | (none yet) |
| `materialize` | catalog build + seams | (none yet) |

## Public API

`materialize_account_catalog`,
`AccountCatalogStores`,
`AccountMembershipDescriptor`,
`AccountCatalog`, `HostAccountDescriptor`,
`CanonicalAccountIdentity`,
`CanonicalIdentityGraph`, selection
persistence, and the view helpers.

## How to verify

```sh
cargo nextest run -p jackin-usage-host-accounts
cargo clippy -p jackin-usage-host-accounts --all-targets -- -D warnings
```
