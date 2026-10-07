// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account catalog and selection persistence.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use jackin_protocol::control::FocusedUsageView;
use serde::{Deserialize, Serialize};

use jackin_usage_provider_core::atomic_write_usage_json;

use super::super::HostSurfaceId;
use super::{AccountLifecycle, AccountProvenance, CanonicalAccountIdentity};

/// One account known for a host surface (current broker state or durable history).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAccountDescriptor {
    pub surface_id: String,
    /// Overview Provider cell; account children leave ownership to the parent.
    pub provider_column_label: String,
    pub account_key: String,
    pub account_label: String,
    pub plan_label: Option<String>,
    pub selected: bool,
    pub lifecycle: String,
    pub lifecycle_label: String,
    pub provenance: Vec<String>,
    pub provenance_label: String,
    pub plan_or_status_label: String,
    pub remaining_percent: Option<u8>,
    pub remaining_label: String,
    pub headline: String,
    pub reset_label: Option<String>,
    /// Non-optional Overview display value (`—` when unknown).
    pub reset_display_label: String,
    pub exact_reset: Option<String>,
    pub status_word: String,
    pub status_label: String,
    pub severity: String,
    pub updated_label: String,
    pub last_error: Option<String>,
    pub dimmed: bool,
    /// Complete combined Overview account-row announcement.
    pub accessibility_label: String,
}

/// Internal source-complete account record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountCatalogEntry {
    pub identity: CanonicalAccountIdentity,
    pub account_key: String,
    pub account_label: String,
    pub username: Option<String>,
    pub plan_label: Option<String>,
    pub provenance: BTreeSet<AccountProvenance>,
    pub discovery_provenance: BTreeSet<String>,
    pub lifecycle: AccountLifecycle,
    pub view: FocusedUsageView,
    pub fetched_at_epoch: i64,
}

/// One materialization of every durable/shared/live source.
#[derive(Debug, Default)]
pub(crate) struct AccountCatalog {
    pub(crate) entries: BTreeMap<(HostSurfaceId, String), AccountCatalogEntry>,
    pub(crate) provider_states: BTreeMap<HostSurfaceId, FocusedUsageView>,
}

impl AccountCatalog {
    pub(crate) fn entries_for_surface(&self, surface: HostSurfaceId) -> Vec<&AccountCatalogEntry> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .filter_map(|((candidate, _), entry)| (*candidate == surface).then_some(entry))
            .collect();
        entries.sort_by(|a, b| {
            lifecycle_rank(a.lifecycle)
                .cmp(&lifecycle_rank(b.lifecycle))
                .then(a.account_label.cmp(&b.account_label))
                .then(a.account_key.cmp(&b.account_key))
        });
        entries
    }

    pub(crate) fn entry(&self, surface: HostSurfaceId, key: &str) -> Option<&AccountCatalogEntry> {
        self.entries.get(&(surface, key.to_owned()))
    }

    pub(crate) fn provider_state(&self, surface: HostSurfaceId) -> Option<&FocusedUsageView> {
        self.provider_states.get(&surface)
    }

    pub(crate) fn preferred_current_key(&self, surface: HostSurfaceId) -> Option<String> {
        self.entries_for_surface(surface)
            .into_iter()
            .filter(|entry| entry.lifecycle == AccountLifecycle::Current)
            .min_by_key(|entry| i32::from(!entry.provenance.contains(&AccountProvenance::LiveHost)))
            .map(|entry| entry.account_key.clone())
    }
}

pub(crate) fn lifecycle_rank(lifecycle: AccountLifecycle) -> u8 {
    match lifecycle {
        AccountLifecycle::Current => 0,
        AccountLifecycle::Historical => 1,
        AccountLifecycle::ProviderPresenceOnly => 2,
    }
}

/// Persist selected account keys: `surface_id -> account_key`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct SelectedAccountsFile {
    selected: HashMap<String, String>,
}

pub(crate) fn selected_accounts_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join(super::super::HOST_USAGE_STATE_REL)
        .join("selected-accounts.json")
}

pub(crate) fn load_selected_accounts(path: &Path) -> HashMap<String, String> {
    let Ok(bytes) = fs::read(path) else {
        return HashMap::new();
    };
    serde_json::from_slice::<SelectedAccountsFile>(&bytes)
        .map(|doc| doc.selected)
        .unwrap_or_default()
}

pub(crate) fn save_selected_accounts(
    path: &Path,
    selected: &HashMap<String, String>,
) -> Result<(), String> {
    let doc = SelectedAccountsFile {
        selected: selected.clone(),
    };
    let json = serde_json::to_string_pretty(&doc)
        .map_err(|err| format!("serialize selected-accounts: {err}"))?;
    atomic_write_usage_json(path, &json).map_err(|err| format!("write selected-accounts: {err}"))
}
