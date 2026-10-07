# jackin-usage-credential-resolver

Cached provider credential
resolution plus coordinator-side
vendor snapshot dispatch.

## What this crate owns

- Resolver (`resolver`):
  `CachedProviderCredentialResolver`
  behind `ProviderCredentialSecretSource`.
- Dispatch (`dispatch`): the
  `CredentialSnapshotVendors`
  arms over the T3 vendor crates.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`resolver.rs`](src/resolver.rs) | secret cache | `tests/` |
| [`dispatch.rs`](src/dispatch.rs) | vendor arms | via resolver |

## Public API

`CachedProviderCredentialResolver`,
`provider_credential_snapshot`.

## How to verify

```sh
cargo nextest run -p jackin-usage-credential-resolver
cargo clippy -p jackin-usage-credential-resolver --all-targets -- -D warnings
```
