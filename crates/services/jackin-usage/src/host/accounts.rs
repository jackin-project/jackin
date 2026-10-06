// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical, multi-source account inventory for the host runtime.

mod catalog;
mod identity;
mod lifecycle;
mod materialize;
mod views;

pub use catalog::HostAccountDescriptor;
pub(crate) use catalog::{
    AccountCatalog, AccountCatalogEntry, lifecycle_rank, load_selected_accounts,
    save_selected_accounts, selected_accounts_path,
};
pub(crate) use identity::CanonicalIdentityGraph;
pub use identity::{CanonicalAccountIdentity, CanonicalAccountSubject};
pub use lifecycle::{AccountLifecycle, AccountProvenance};
pub(crate) use materialize::materialize_account_catalog;
pub use views::{
    account_key_for_view, canonical_account_id_for_view, min_remaining, short_account_identity,
};
pub(crate) use views::{stable_account_label, surface_for_view};
