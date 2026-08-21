// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical, multi-source account inventory for the host runtime.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use jackin_core::account_key_hash;
use jackin_protocol::control::{FocusedUsageView, UsageConfidence};
use serde::{Deserialize, Serialize};

use crate::usage::atomic_write_usage_json;
use crate::usage_snapshot_store;

use super::HostSurfaceId;

/// Identity evidence used below the operator-visible account label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CanonicalAccountSubject {
    /// Provider-issued account or organization identifier, when available.
    ProviderId(String),
    /// Provider-authenticated stable non-secret handle when no stronger ID exists.
    ProviderStableHandle(String),
}

/// Exact account identity. Probe-routing slugs and source paths are excluded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanonicalAccountIdentity {
    /// Exact provider ownership.
    pub surface: HostSurfaceId,
    /// Provider-owned subject.
    pub subject: CanonicalAccountSubject,
}

/// Proven capability aliases and their complete typed canonical evidence.
#[derive(Debug, Default, Clone)]
pub(super) struct CanonicalIdentityGraph {
    evidence_by_id: BTreeMap<String, CanonicalAccountIdentity>,
    aliases: BTreeMap<String, String>,
}

impl CanonicalIdentityGraph {
    pub(super) fn resolve_alias(
        &mut self,
        capability_id: &str,
        identity: &CanonicalAccountIdentity,
    ) -> Result<String, String> {
        let canonical_id = identity.canonical_id_v1();
        if self
            .evidence_by_id
            .get(&canonical_id)
            .is_some_and(|existing| existing != identity)
        {
            return Err("canonical account identity collision".to_owned());
        }
        if self
            .aliases
            .get(capability_id)
            .is_some_and(|existing| existing != &canonical_id)
        {
            return Err("canonical account alias collision".to_owned());
        }

        // Commit both sides only after every invariant succeeds. Replays are
        // idempotent and cannot expose a half-written alias transition.
        self.evidence_by_id
            .insert(canonical_id.clone(), identity.clone());
        self.aliases
            .insert(capability_id.to_owned(), canonical_id.clone());
        Ok(canonical_id)
    }
}

impl CanonicalAccountIdentity {
    pub(super) fn from_view(surface: HostSurfaceId, view: &FocusedUsageView) -> Option<Self> {
        if surface_for_view(view) != Some(surface)
            || matches!(view.confidence, UsageConfidence::PresenceOnly)
        {
            return None;
        }
        let label = stable_account_label(&view.account.account_label)?;
        Some(Self {
            surface,
            subject: CanonicalAccountSubject::ProviderStableHandle(label.to_owned()),
        })
    }

    pub(super) fn account_key(&self) -> String {
        let subject = match &self.subject {
            CanonicalAccountSubject::ProviderId(id)
            | CanonicalAccountSubject::ProviderStableHandle(id) => id,
        };
        account_key_hash(self.surface.account_provider_label(), subject)
    }

    pub(super) fn canonical_id_v1(&self) -> String {
        let evidence = match &self.subject {
            CanonicalAccountSubject::ProviderId(id) => {
                format!("canonical-account-v1:provider-id:{}", id.trim())
            }
            CanonicalAccountSubject::ProviderStableHandle(handle) => format!(
                "canonical-account-v1:stable-handle:{}",
                normalize_stable_handle(handle)
            ),
        };
        account_key_hash(self.surface.provider_id(), &evidence)
    }
}

fn normalize_stable_handle(handle: &str) -> String {
    handle.trim().to_lowercase()
}

/// Account lifecycle is independent from snapshot freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountLifecycle {
    /// Affirmed by current broker discovery or a current host projection.
    Current,
    /// Available only as durable/stale history.
    Historical,
    /// Credential presence without authenticated account identity.
    ProviderPresenceOnly,
}

impl AccountLifecycle {
    /// Stable DTO label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Historical => "historical",
            Self::ProviderPresenceOnly => "provider_presence_only",
        }
    }
}

/// Non-secret places that contributed evidence for one canonical account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccountProvenance {
    /// Current config-derived credential source.
    ConfiguredSource,
    /// Active host credential/login.
    LiveHost,
    /// Durable last-good snapshot.
    DurableHistory,
}

impl AccountProvenance {
    /// Rust-owned user-facing provenance copy.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::ConfiguredSource => "Configured source",
            Self::LiveHost => "Live host",
            Self::DurableHistory => "History",
        }
    }
}

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
pub(super) struct AccountCatalogEntry {
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
pub(super) struct AccountCatalog {
    entries: BTreeMap<(HostSurfaceId, String), AccountCatalogEntry>,
    provider_states: BTreeMap<HostSurfaceId, FocusedUsageView>,
}

impl AccountCatalog {
    pub(super) fn entries_for_surface(&self, surface: HostSurfaceId) -> Vec<&AccountCatalogEntry> {
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

    pub(super) fn entry(&self, surface: HostSurfaceId, key: &str) -> Option<&AccountCatalogEntry> {
        self.entries.get(&(surface, key.to_owned()))
    }

    pub(super) fn provider_state(&self, surface: HostSurfaceId) -> Option<&FocusedUsageView> {
        self.provider_states.get(&surface)
    }

    pub(super) fn preferred_current_key(&self, surface: HostSurfaceId) -> Option<String> {
        self.entries_for_surface(surface)
            .into_iter()
            .filter(|entry| entry.lifecycle == AccountLifecycle::Current)
            .min_by_key(|entry| i32::from(!entry.provenance.contains(&AccountProvenance::LiveHost)))
            .map(|entry| entry.account_key.clone())
    }
}

fn lifecycle_rank(lifecycle: AccountLifecycle) -> u8 {
    match lifecycle {
        AccountLifecycle::Current => 0,
        AccountLifecycle::Historical => 1,
        AccountLifecycle::ProviderPresenceOnly => 2,
    }
}

/// Persist selected account keys: `surface_id -> account_key`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct SelectedAccountsFile {
    selected: HashMap<String, String>,
}

pub(super) fn selected_accounts_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join(super::HOST_USAGE_STATE_REL)
        .join("selected-accounts.json")
}

pub(super) fn load_selected_accounts(path: &Path) -> HashMap<String, String> {
    let Ok(bytes) = fs::read(path) else {
        return HashMap::new();
    };
    serde_json::from_slice::<SelectedAccountsFile>(&bytes)
        .map(|doc| doc.selected)
        .unwrap_or_default()
}

pub(super) fn save_selected_accounts(
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

/// Stable key for a focused usage view after exact provider canonicalization.
#[must_use]
pub fn account_key_for_view(view: &FocusedUsageView) -> Option<String> {
    let surface = surface_for_view(view)?;
    CanonicalAccountIdentity::from_view(surface, view).map(|identity| identity.account_key())
}

/// Canonical V1 identity for a focused usage view.
#[must_use]
pub fn canonical_account_id_for_view(view: &FocusedUsageView) -> Option<String> {
    let surface = surface_for_view(view)?;
    CanonicalAccountIdentity::from_view(surface, view).map(|identity| identity.canonical_id_v1())
}

/// Compact identity for status chips (email local-part when possible).
#[must_use]
pub fn short_account_identity(account_label: &str) -> String {
    let Some(trimmed) = stable_account_label(account_label) else {
        return String::new();
    };
    if let Some((local, _)) = trimmed.split_once('@')
        && !local.is_empty()
    {
        return local.to_owned();
    }
    if trimmed.chars().count() > 12 {
        return trimmed.chars().take(10).collect::<String>() + "…";
    }
    trimmed.to_owned()
}

fn stable_account_label(account_label: &str) -> Option<&str> {
    let label = account_label.trim();
    if label.is_empty()
        || label.eq_ignore_ascii_case("account unavailable")
        || label.eq_ignore_ascii_case("unknown")
        || label.eq_ignore_ascii_case("current host login")
        || label.eq_ignore_ascii_case("local amp auth")
    {
        None
    } else {
        Some(label)
    }
}

/// Min remaining across numeric buckets.
#[must_use]
pub fn min_remaining(view: &FocusedUsageView) -> Option<u8> {
    view.buckets
        .iter()
        .filter_map(|bucket| bucket.remaining_percent)
        .min()
}

/// Closed provider-alias parser. It never performs containment matching.
#[must_use]
pub(super) fn surface_for_view(view: &FocusedUsageView) -> Option<HostSurfaceId> {
    HostSurfaceId::from_provider_alias(&view.account.provider_label)
}

/// Build one catalog by scanning each external source exactly once.
pub(super) fn materialize_account_catalog(
    live_views: &[(HostSurfaceId, FocusedUsageView, bool)],
    discovered_views: &BTreeMap<(HostSurfaceId, String), FocusedUsageView>,
    discovered_provider_views: &BTreeMap<HostSurfaceId, FocusedUsageView>,
    store_path: &Path,
    membership: Option<&[super::DiscoveredAccountDescriptor]>,
) -> Result<AccountCatalog, String> {
    let mut catalog = AccountCatalog::default();
    let include_external: BTreeMap<_, _> = live_views
        .iter()
        .map(|(surface, _, include)| (*surface, *include))
        .collect();

    if store_path.exists() {
        for stored in usage_snapshot_store::load_all_account_usage_views(
            store_path,
            chrono::Utc::now().timestamp(),
        )? {
            let Some(surface) = surface_for_view(&stored.view) else {
                continue;
            };
            if !include_external.get(&surface).copied().unwrap_or(true) {
                continue;
            }
            let identity = membership_identity(membership, surface, &stored.view);
            if membership.is_some() && identity.is_none() {
                continue;
            }
            merge_view(
                &mut catalog,
                surface,
                stored.view,
                if membership.is_some() {
                    AccountLifecycle::Current
                } else {
                    AccountLifecycle::Historical
                },
                AccountProvenance::DurableHistory,
                identity,
            );
        }
    }

    for (surface, view, _) in live_views {
        catalog.provider_states.insert(*surface, view.clone());
        let identity = membership_identity(membership, *surface, view);
        if membership.is_some() && identity.is_none() {
            continue;
        }
        merge_view(
            &mut catalog,
            *surface,
            view.clone(),
            AccountLifecycle::Current,
            AccountProvenance::LiveHost,
            identity,
        );
    }
    catalog.provider_states.extend(
        discovered_provider_views
            .iter()
            .map(|(surface, view)| (*surface, view.clone())),
    );
    if let Some(membership) = membership {
        for ((surface, account_key), view) in discovered_views {
            let Some(account) = membership.iter().find(|account| {
                account.surface_id == surface.id() && account.account_key == *account_key
            }) else {
                continue;
            };
            merge_view(
                &mut catalog,
                *surface,
                view.clone(),
                AccountLifecycle::Current,
                AccountProvenance::ConfiguredSource,
                Some(account.identity.clone()),
            );
        }
    }
    if let Some(membership) = membership {
        merge_discovered_placeholders(&mut catalog, membership);
    }
    Ok(catalog)
}

fn membership_identity(
    membership: Option<&[super::DiscoveredAccountDescriptor]>,
    surface: HostSurfaceId,
    view: &FocusedUsageView,
) -> Option<CanonicalAccountIdentity> {
    let membership = membership?;
    // Existing snapshots retain their pre-V1 routing key during additive
    // migration. Only discovery supplies canonical evidence; display-label
    // comparison never promotes a snapshot into membership.
    let routing_key = CanonicalAccountIdentity::from_view(surface, view)?.account_key();
    membership
        .iter()
        .find(|account| account.surface_id == surface.id() && account.account_key == routing_key)
        .map(|account| account.identity.clone())
}

fn merge_discovered_placeholders(
    catalog: &mut AccountCatalog,
    membership: &[super::DiscoveredAccountDescriptor],
) {
    for account in membership {
        let surface = account.identity.surface;
        let key = (surface, account.account_key.clone());
        if let Some(entry) = catalog.entries.get_mut(&key) {
            entry
                .discovery_provenance
                .extend(account.provenance.iter().cloned());
            entry.lifecycle = AccountLifecycle::Current;
            continue;
        }
        let mut view =
            FocusedUsageView::refreshing(surface.provider_label(), chrono::Utc::now().timestamp());
        view.focused_agent = Some(surface.agent_slug().to_owned());
        view.account.account_label = account.account_label.clone();
        view.updated_label = "Not refreshed".to_owned();
        view.last_error = None;
        catalog.entries.insert(
            key,
            AccountCatalogEntry {
                identity: account.identity.clone(),
                account_key: account.account_key.clone(),
                account_label: account.account_label.clone(),
                username: None,
                plan_label: None,
                provenance: BTreeSet::new(),
                discovery_provenance: account.provenance.iter().cloned().collect(),
                lifecycle: AccountLifecycle::Current,
                fetched_at_epoch: view.fetched_at_epoch,
                view,
            },
        );
    }
}

fn merge_view(
    catalog: &mut AccountCatalog,
    surface: HostSurfaceId,
    view: FocusedUsageView,
    lifecycle: AccountLifecycle,
    provenance: AccountProvenance,
    forced_identity: Option<CanonicalAccountIdentity>,
) {
    let Some(identity) =
        forced_identity.or_else(|| CanonicalAccountIdentity::from_view(surface, &view))
    else {
        return;
    };
    let account_key = identity.account_key();
    let map_key = (surface, account_key.clone());
    let fetched_at_epoch = view.fetched_at_epoch;
    let entry = catalog.entries.entry(map_key).or_insert_with(|| {
        let mut sources = BTreeSet::new();
        sources.insert(provenance);
        AccountCatalogEntry {
            identity: identity.clone(),
            account_key: account_key.clone(),
            account_label: view.account.account_label.trim().to_owned(),
            username: view.account.username.clone(),
            plan_label: view.account.plan_label.clone(),
            provenance: sources,
            discovery_provenance: BTreeSet::new(),
            lifecycle,
            fetched_at_epoch,
            view: view.clone(),
        }
    });
    entry.provenance.insert(provenance);
    let replace = lifecycle_rank(lifecycle) < lifecycle_rank(entry.lifecycle)
        || (lifecycle == entry.lifecycle && fetched_at_epoch >= entry.fetched_at_epoch);
    if replace {
        entry.identity = identity;
        entry.account_label = view.account.account_label.trim().to_owned();
        entry.username = view.account.username.clone();
        entry.plan_label = view.account.plan_label.clone();
        entry.lifecycle = lifecycle;
        entry.fetched_at_epoch = fetched_at_epoch;
        entry.view = view;
    }
}
