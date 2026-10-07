# jackin-usage-credential-snapshots

Configured-provider credential snapshot dispatch: routes a
`(surface, key, secret)` probe to the owning vendor arm while the
caller retains the credential. The secret is never returned or
persisted.

## What this crate owns

- Dispatch (`dispatch`): surface routing plus the vendor-free
  `codex`/`cursor` typed gaps and the explicitly blocked surfaces.
- Vendor seam (`CredentialSnapshotVendors`): the nine per-vendor
  arms, implemented once by the `jackin-usage` coordinator — a T3
  crate cannot name its T3 siblings, so the arms live with the
  caller.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | [`tests.rs`](src/tests.rs) |
| [`dispatch.rs`](src/dispatch.rs) | routing + vendor seam | mock-vendor routing |

## Public API

`provider_credential_snapshot`,
`provider_credential_snapshot_with_rate_limit`, and
`CredentialSnapshotVendors`.

## How to verify

```sh
cargo nextest run -p jackin-usage-credential-snapshots
cargo clippy -p jackin-usage-credential-snapshots --all-targets -- -D warnings
```
