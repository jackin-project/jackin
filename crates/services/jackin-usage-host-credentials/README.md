# jackin-usage-host-credentials

Host credential domain types: opaque handles, env
resolution outcomes, and the governed registry-name
mapping shared by discovery, the broker, and the host
credential cache. Secrets never cross this boundary.

## What this crate owns

- Handles (`OpaqueCredentialHandle`,
  `ForwardedUsageAccount`): non-secret
  adapter-owned identifiers.
- Resolution (`ProviderCredentialEnvResolver`
  + outcomes): the env resolution seam and
  its result types.
- Governed names
  (`governed_name_for_account_alias`):
  discovery-alias to registry mapping.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | (plain types) |
| [`credentials.rs`](src/credentials.rs) | domain types | (plain types) |

## Public API

`ProviderCredentialEnvResolver`,
`ProviderCredentialEnvResolution`,
`ProviderCredentialEnvOutcome`,
`ProviderCredentialIdentityOutcome`,
`ProviderCredentialRefreshOutcome`,
`ProviderCredentialSourceMaterial`,
`OpaqueCredentialHandle`,
`ForwardedUsageAccount`,
`UsageCredentialKind`,
`governed_name_for_account_alias`.

## How to verify

```sh
cargo nextest run -p jackin-usage-host-credentials
cargo clippy -p jackin-usage-host-credentials --all-targets -- -D warnings
```
