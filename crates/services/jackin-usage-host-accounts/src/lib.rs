//! jackin-usage-host-accounts: canonical host account inventory.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`materialize_account_catalog`] — catalog materialization.
//!
//! Canonical, multi-source account inventory for the host runtime.
//! Durable reads and current-membership evidence arrive through the
//! [`AccountCatalogStores`] / [`AccountMembershipDescriptor`] seams,
//! implemented once by the T4 host: the snapshot store is a same-tier
//! sibling and membership descriptors are T4 discovery types.

mod catalog;
mod identity;
mod lifecycle;
mod materialize;
mod views;

pub(crate) use catalog::lifecycle_rank;
pub use catalog::{
    AccountCatalog, AccountCatalogEntry, HostAccountDescriptor, load_selected_accounts,
    save_selected_accounts, selected_accounts_path,
};
pub use identity::{CanonicalAccountIdentity, CanonicalAccountSubject, CanonicalIdentityGraph};
pub use lifecycle::{AccountLifecycle, AccountProvenance};
pub use materialize::{
    AccountCatalogStores, AccountMembershipDescriptor, materialize_account_catalog,
};
pub use views::{
    account_key_for_view, canonical_account_id_for_view, min_remaining, short_account_identity,
};
pub(crate) use views::{stable_account_label, surface_for_view};
