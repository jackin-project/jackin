// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! T4 implementations of the account-catalog seams.
//!
//! The snapshot store is a same-tier sibling of the accounts crate and
//! membership descriptors are discovery types, so both seams are
//! implemented here, where the T4 host can name every side.

use std::path::Path;

use jackin_protocol::control::FocusedUsageView;
use jackin_usage_host_accounts::{
    AccountCatalogStores, AccountMembershipDescriptor, CanonicalAccountIdentity,
};

use super::DiscoveredAccountDescriptor;
use crate::usage_snapshot_store;

/// Same-tier snapshot reads for catalog materialization.
pub(crate) struct HostAccountCatalogStores;

impl AccountCatalogStores for HostAccountCatalogStores {
    fn load_stored_views(
        &self,
        store_path: &Path,
        now_epoch: i64,
    ) -> Result<Vec<FocusedUsageView>, String> {
        usage_snapshot_store::load_all_account_usage_views(store_path, now_epoch)
            .map(|rows| rows.into_iter().map(|stored| stored.view).collect())
    }
}

impl AccountMembershipDescriptor for DiscoveredAccountDescriptor {
    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn account_key(&self) -> &str {
        &self.account_key
    }

    fn account_label(&self) -> &str {
        &self.account_label
    }

    fn provenance(&self) -> &[String] {
        &self.provenance
    }

    fn identity(&self) -> CanonicalAccountIdentity {
        self.identity.clone()
    }
}
