//! jackin-usage-credential-snapshots: configured-provider credential dispatch.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`provider_credential_snapshot`] — one explicit snapshot.
//!
//! Tier-3 probe body used by tier-4 protected-source adapters; the secret is
//! never returned or persisted. Vendor calls stay behind
//! [`CredentialSnapshotVendors`], implemented once by the `jackin-usage`
//! coordinator: a T3 crate cannot name its T3 siblings (dependency direction
//! is strictly lower-tier), so the per-vendor arms live with the caller and
//! this crate owns only the surface routing.

mod dispatch;

pub use dispatch::{
    CredentialSnapshotVendors, provider_credential_snapshot,
    provider_credential_snapshot_with_rate_limit,
};

#[cfg(test)]
mod tests;
