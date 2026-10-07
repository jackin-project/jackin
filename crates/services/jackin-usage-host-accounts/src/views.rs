// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! View-derived account keys.

use jackin_protocol::control::FocusedUsageView;

use super::CanonicalAccountIdentity;
use jackin_usage_host_presentation::HostSurfaceId;

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

pub(crate) fn stable_account_label(account_label: &str) -> Option<&str> {
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
pub(crate) fn surface_for_view(view: &FocusedUsageView) -> Option<HostSurfaceId> {
    HostSurfaceId::from_provider_alias(&view.account.provider_label)
}
