//! jackin-usage-host-credentials: host credential domain types.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`ProviderCredentialEnvResolver`] — env resolution seam.
//!
//! Opaque credential handles, env resolution outcomes, and the governed
//! registry-name mapping shared by discovery, the broker, and the host
//! credential cache. Secrets never cross this boundary: resolvers retain
//! values internally and hand out opaque handles only.

mod credentials;

pub use credentials::{
    ForwardedUsageAccount, OpaqueCredentialHandle, ProviderCredentialEnvOutcome,
    ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
    ProviderCredentialIdentityOutcome, ProviderCredentialRefreshOutcome,
    ProviderCredentialSourceMaterial, UsageCredentialKind, governed_name_for_account_alias,
};

pub use jackin_usage_provider_core::ProviderFailureMetadata;
