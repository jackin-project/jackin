// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage cache key derivation and matching.

use jackin_core::account_key_hash;
use std::collections::HashMap;

use jackin_protocol::control::FocusedUsageView;

use super::{CachedUsage, UsageSurface, resolve_surface};

pub fn canonical_usage_cache_key(agent: &str, focused_provider: Option<&str>) -> String {
    let surface = resolve_surface(agent, focused_provider);
    if surface == UsageSurface::Unsupported {
        return format!("{agent}:{}", focused_provider.unwrap_or_default());
    }
    surface.label().to_owned()
}

pub fn usage_cache_key_for_view(
    agent: &str,
    focused_provider: Option<&str>,
    view: &FocusedUsageView,
) -> String {
    let base = canonical_usage_cache_key(agent, focused_provider);
    let Some(label) = stable_cache_account_label(&view.account.account_label) else {
        return base;
    };
    let surface = resolve_surface(agent, focused_provider);
    let surface_id = surface.id().unwrap_or(agent);
    let evidence = format!(
        "usage-cache-account-v1:{}:{}",
        length_prefixed(surface_id),
        length_prefixed(&label),
    );
    let hash = account_key_hash("usage-cache-account-v1", &evidence);
    let hash = hash.strip_prefix("sha256:").unwrap_or(&hash);
    format!("{base}:account-{hash}")
}

pub fn usage_cache_key_for_broker_account(
    agent: &str,
    focused_provider: Option<&str>,
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> String {
    format!(
        "{}:account-id-v1:{}:{}",
        canonical_usage_cache_key(agent, focused_provider),
        capability.surface_id,
        capability.account_id,
    )
}

pub fn stable_cache_account_label(label: &str) -> Option<String> {
    let label = label.trim();
    if label.is_empty()
        || label.eq_ignore_ascii_case("account unavailable")
        || label.eq_ignore_ascii_case("unknown")
        || label.eq_ignore_ascii_case("current host login")
        || label.eq_ignore_ascii_case("refreshing")
    {
        None
    } else {
        Some(label.to_lowercase())
    }
}

pub(crate) fn length_prefixed(value: &str) -> String {
    format!("{}:{value}", value.len())
}

pub(crate) fn cache_view_matches_target(
    view: &FocusedUsageView,
    agent: &str,
    focused_provider: Option<&str>,
) -> bool {
    let target_surface = resolve_surface(agent, focused_provider);
    let view_surface = resolve_surface(
        view.focused_agent.as_deref().unwrap_or_default(),
        view.focused_provider
            .as_deref()
            .or(Some(view.account.provider_label.as_str())),
    );
    if target_surface == UsageSurface::Unsupported {
        view.focused_agent.as_deref() == Some(agent)
            && view.focused_provider.as_deref() == focused_provider
    } else {
        view_surface == target_surface
    }
}

pub(crate) fn cache_key_matches_target(
    key: &str,
    agent: &str,
    focused_provider: Option<&str>,
) -> bool {
    let base = canonical_usage_cache_key(agent, focused_provider);
    key == base || key.starts_with(&format!("{base}:account-"))
}

pub fn cached_usage_for_target<'a>(
    snapshots: &'a HashMap<String, CachedUsage>,
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<&'a CachedUsage> {
    let key = cached_usage_key_for_target(snapshots, agent, focused_provider)?;
    snapshots.get(&key)
}

pub fn cached_usage_for_capability<'a>(
    snapshots: &'a HashMap<String, CachedUsage>,
    agent: &str,
    focused_provider: Option<&str>,
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> Option<&'a CachedUsage> {
    let key = usage_cache_key_for_broker_account(agent, focused_provider, capability);
    snapshots.get(&key)
}

pub fn cached_usage_key_for_target(
    snapshots: &HashMap<String, CachedUsage>,
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<String> {
    snapshots
        .iter()
        .filter(|(key, cached)| {
            cache_key_matches_target(key, agent, focused_provider)
                || cache_view_matches_target(&cached.view, agent, focused_provider)
        })
        .max_by(|(left_key, left), (right_key, right)| {
            left.view
                .fetched_at_epoch
                .cmp(&right.view.fetched_at_epoch)
                .then_with(|| left_key.cmp(right_key))
        })
        .map(|(key, _)| key.clone())
}
