// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host implementation of the account-catalog stores seam.
//!
//! The snapshot store is a same-tier sibling of the accounts crate, so
//! the seam is implemented here, where the host can name every side.
//! The membership-descriptor seam lives with its discovery types in
//! `jackin-usage-discovery`.

use std::path::Path;

use jackin_protocol::control::FocusedUsageView;
use jackin_usage_host_accounts::AccountCatalogStores;

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
