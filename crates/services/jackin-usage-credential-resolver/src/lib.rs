mod dispatch;
mod resolver;

pub use dispatch::provider_credential_snapshot;
pub use resolver::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};
