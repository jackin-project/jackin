// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host desktop inventory projection types.

use super::{
    HostAccountDescriptor, HostProviderGlanceRow, HostSurfaceDescriptor, UsageDiscoveryDiagnostic,
};

use jackin_protocol::control::{FocusedUsageView, UsageIdentityPresentation};

/// Provider state when detection succeeds without a stable account identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProviderState {
    pub status_word: String,
    pub status_label: String,
    pub updated_label: String,
    pub last_error: Option<String>,
    pub is_refreshing: bool,
}

/// One Rust-ordered Desktop provider group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProviderGroup {
    pub surface_id: String,
    pub display_label: String,
    pub icon_key: String,
    pub fallback_glyph: String,
    pub usage_url: Option<String>,
    pub account_column_label: String,
    pub plan_or_status_label: String,
    pub remaining_label: String,
    pub reset_display_label: String,
    pub accessibility_label: String,
    pub accounts: Vec<HostAccountDescriptor>,
    pub empty_state: Option<HostDesktopProviderState>,
}

/// Atomic account inventory consumed by jackin❯ desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopInventory {
    pub groups: Vec<HostDesktopProviderGroup>,
}

/// One provider group plus its selected, fully-presented usage snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProviderProjection {
    pub group: HostDesktopProviderGroup,
    pub selected_account_route: HostSelectedAccountRoute,
    pub selected_usage: FocusedUsageView,
    pub identity: UsageIdentityPresentation,
    pub is_updating: bool,
}

/// Exact persisted-account route and its membership resolution for one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSelectedAccountRoute {
    /// No persisted account choice exists for this provider.
    Unselected,
    /// The persisted key exists, but cold discovery has not established membership yet.
    Resolving { account_key: String },
    /// The exact persisted key is present in the account catalog.
    Available { account_key: String },
    /// Discovery completed and the exact persisted key is absent from the catalog.
    Unavailable {
        account_key: String,
        notice: &'static str,
    },
}

/// One immutable Desktop state boundary, produced while the runtime is locked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopProjection {
    pub generation: u64,
    pub refresh_in_progress: bool,
    pub error_message: Option<String>,
    pub next_refresh_label: String,
    pub surfaces: Vec<HostSurfaceDescriptor>,
    pub providers: Vec<HostDesktopProviderProjection>,
    pub glance_rows: Vec<HostProviderGlanceRow>,
    pub status_bar_glance_rows: Vec<HostProviderGlanceRow>,
    pub diagnostics: Vec<UsageDiscoveryDiagnostic>,
}
