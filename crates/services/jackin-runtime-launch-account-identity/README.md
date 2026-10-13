# jackin-runtime-launch-account-identity

Launch account
admission identity:
generation leases,
credential publication,
fingerprints.

## What this crate owns

- Identity
  (`account_identity`):
  `AccountConfigRevision`,
  `record_account_configuration`,
  `write_account_credentials` —
  lease + publication.
- Fingerprints
  (`account_identity`):
  `account_configuration_fingerprint`,
  `account_configuration_matches`,
  `account_admission_matches`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`account_identity.rs`](src/account_identity.rs) | identity | `account_identity/tests/` |

## Public API

`account_identity::AccountConfigRevision`,
`account_identity::record_account_configuration`,
`account_identity::write_account_credentials`,
`account_identity::account_configuration_fingerprint`,
`account_identity::account_configuration_matches`,
`account_identity::account_admission_matches`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-account-identity
cargo clippy -p jackin-runtime-launch-account-identity --all-targets -- -D warnings
```
