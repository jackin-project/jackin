//! jackin-usage-credential-resolver: cached provider credential resolution.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`CachedProviderCredentialResolver`] — opaque-handle cache.
//!
//! Process-scoped secret cache behind [`ProviderCredentialSecretSource`]
//! adapters, plus the coordinator-side `CredentialSnapshotVendors`
//! implementation over the T3 vendor crates used by the refresh path.
//! Secrets never cross this boundary except into opaque handles: resolvers
//! retain only fingerprints and handles after the snapshot is built.

mod dispatch;
mod resolver;

pub use dispatch::provider_credential_snapshot;
pub use resolver::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};
