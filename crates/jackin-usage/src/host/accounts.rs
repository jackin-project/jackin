// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Multi-account support for the host menu-bar runtime.
//!
//! jackin❯ already scopes shared usage by OAuth identity
//! (`Claude#…` / `Codex#…`). Desktop lists every known account for a surface
//! (live host login + durable store + shared snapshots from containers) and
//! lets the operator select which account drives the detail card / snapshot.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use jackin_core::account_key_hash;
use jackin_protocol::control::FocusedUsageView;
use serde::{Deserialize, Serialize};

use crate::usage::shared_usage_snapshots_dir;
use crate::usage_snapshot_store;

use super::HostSurfaceId;

/// One account known for a host surface (live, store, or shared snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAccountDescriptor {
    /// Host surface id (`claude`, `codex`, …).
    pub surface_id: String,
    /// Stable account key (`account_key_hash` of provider + label).
    pub account_key: String,
    /// Operator-visible account identity (email / handle).
    pub account_label: String,
    /// Plan when known.
    pub plan_label: Option<String>,
    /// Whether this account is selected for detail + snapshot for the surface.
    pub selected: bool,
    /// Tightest remaining % across buckets when numeric.
    pub remaining_percent: Option<u8>,
    /// Storage status word for the account view.
    pub status_word: String,
}

/// Persist selected account keys: `surface_id -> account_key`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct SelectedAccountsFile {
    /// Selected account key per surface id.
    selected: HashMap<String, String>,
}

/// Path for selected-account prefs under the menu-bar data dir.
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create selected-accounts dir: {err}"))?;
    }
    let doc = SelectedAccountsFile {
        selected: selected.clone(),
    };
    let json = serde_json::to_vec_pretty(&doc)
        .map_err(|err| format!("serialize selected-accounts: {err}"))?;
    fs::write(path, json).map_err(|err| format!("write selected-accounts: {err}"))
}

/// Stable key for a focused usage view (matches snapshot-store hashing).
#[must_use]
pub fn account_key_for_view(view: &FocusedUsageView) -> String {
    account_key_hash(
        &view.account.provider_label,
        &view.account.account_label,
    )
}

/// Compact identity for status chips (email local-part when possible).
#[must_use]
pub fn short_account_identity(account_label: &str) -> String {
    let trimmed = account_label.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("account unavailable")
        || trimmed.eq_ignore_ascii_case("unknown")
    {
        return String::new();
    }
    if let Some((local, _)) = trimmed.split_once('@') {
        if !local.is_empty() {
            return local.to_owned();
        }
    }
    if trimmed.chars().count() > 12 {
        return trimmed.chars().take(10).collect::<String>() + "…";
    }
    trimmed.to_owned()
}

/// Min remaining across numeric buckets.
#[must_use]
pub fn min_remaining(view: &FocusedUsageView) -> Option<u8> {
    view.buckets
        .iter()
        .filter_map(|b| b.remaining_percent)
        .min()
}

/// Map a focused view's provider label onto a host surface id.
#[must_use]
pub(super) fn surface_for_view(view: &FocusedUsageView) -> Option<HostSurfaceId> {
    let provider = view.account.provider_label.as_str();
    for surface in HostSurfaceId::ALL {
        if let Some(label) = surface.provider_label() {
            if provider_matches(label, provider) {
                return Some(*surface);
            }
        }
        // Agent slug match (OpenCode, Amp, …).
        if provider_matches(surface.label(), provider)
            || provider_matches(surface.id(), provider)
            || provider_matches(surface.agent_slug(), provider)
        {
            return Some(*surface);
        }
    }
    None
}

fn provider_matches(a: &str, b: &str) -> bool {
    let na = normalize(a);
    let nb = normalize(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    na == nb
        || na.contains(&nb)
        || nb.contains(&na)
        || (na.contains("openai") && nb.contains("codex"))
        || (na.contains("codex") && nb.contains("openai"))
        || (na.contains("anthropic") && nb.contains("claude"))
        || (na.contains("claude") && nb.contains("anthropic"))
        || (na.contains("xai") && nb.contains("grok"))
        || (na.contains("grok") && nb.contains("xai"))
        || (na.contains("zai") && nb.contains("glm"))
        || (na.contains("glm") && nb.contains("zai"))
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Collect distinct account views for a surface from live + store + shared files.
pub(super) fn collect_account_views(
    surface: HostSurfaceId,
    live: Option<&FocusedUsageView>,
    store_path: &Path,
) -> HashMap<String, FocusedUsageView> {
    let mut by_key: HashMap<String, FocusedUsageView> = HashMap::new();

    if let Some(view) = live {
        let key = account_key_for_view(view);
        if !key.is_empty() {
            by_key.insert(key, view.clone());
        }
    }

    // Durable menu-bar store (survives restarts; multi-account when host login changed).
    if let Ok(identities) = usage_snapshot_store::list_account_identities(store_path) {
        for identity in identities {
            if !surface_matches_provider(surface, &identity.provider) {
                continue;
            }
            if by_key.contains_key(&identity.account_key_hash) {
                continue;
            }
            if let Ok(Some(view)) = usage_snapshot_store::load_account_usage_view(
                store_path,
                &identity.account_key_hash,
                chrono::Utc::now().timestamp(),
            ) {
                by_key.insert(identity.account_key_hash, view);
            }
        }
    }

    // Shared host/container snapshots (different OAuth identities).
    for view in scan_shared_usage_views() {
        let Some(view_surface) = surface_for_view(&view) else {
            continue;
        };
        if view_surface != surface {
            continue;
        }
        let key = account_key_for_view(&view);
        if key.is_empty() {
            continue;
        }
        // Prefer newer fetched_at when colliding.
        match by_key.get(&key) {
            Some(existing) if existing.fetched_at_epoch >= view.fetched_at_epoch => {}
            _ => {
                by_key.insert(key, view);
            }
        }
    }

    by_key
}

fn surface_matches_provider(surface: HostSurfaceId, provider: &str) -> bool {
    if let Some(label) = surface.provider_label() {
        if provider_matches(label, provider) {
            return true;
        }
    }
    provider_matches(surface.label(), provider)
        || provider_matches(surface.id(), provider)
        || provider_matches(surface.agent_slug(), provider)
}

fn scan_shared_usage_views() -> Vec<FocusedUsageView> {
    let dir = shared_usage_snapshots_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("usage-") || !name.ends_with(".snapshot.json") {
            continue;
        }
        let Ok(json) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(view) = serde_json::from_str::<FocusedUsageView>(&json) {
            views.push(view);
        }
    }
    views
}

/// Resolve the view for a selected account key, falling back to live.
pub(super) fn resolve_account_view(
    surface: HostSurfaceId,
    selected_key: Option<&str>,
    live: FocusedUsageView,
    store_path: &Path,
) -> FocusedUsageView {
    let live_key = account_key_for_view(&live);
    let Some(want) = selected_key else {
        return live;
    };
    if want == live_key || want.is_empty() {
        return live;
    }
    let accounts = collect_account_views(surface, Some(&live), store_path);
    accounts.get(want).cloned().unwrap_or(live)
}

