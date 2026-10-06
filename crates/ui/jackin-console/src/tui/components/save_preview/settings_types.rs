// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings save preview types and builders.

use super::settings_save_lines;
use std::collections::BTreeMap;

use crate::tui::auth_config::env_display_map_without_auth_credentials;
use ratatui::text::Line;

use crate::tui::screens::settings::model::{
    GlobalMountsState, SettingsAuthState, SettingsEnvState, SettingsState, SettingsTrustState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSavePreview {
    pub general: SettingsGeneralPreview,
    pub mounts_original: Vec<MountPreviewRow>,
    pub mounts_pending: Vec<MountPreviewRow>,
    pub env_original: SettingsEnvPreview,
    pub env_pending: SettingsEnvPreview,
    pub auth_original: BTreeMap<String, jackin_config::AccountConfig>,
    pub auth_pending: BTreeMap<String, jackin_config::AccountConfig>,
    pub github_original: jackin_config::GithubAuthConfig,
    pub github_pending: jackin_config::GithubAuthConfig,
    pub bindings_original: BTreeMap<jackin_core::Agent, String>,
    pub bindings_pending: BTreeMap<jackin_core::Agent, String>,
    pub trust_original: Vec<TrustPreviewRow>,
    pub trust_pending: Vec<TrustPreviewRow>,
}

pub type ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit> =
    SettingsState<
        GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
        SettingsEnvState<jackin_config::EnvValue, EnvModal>,
        SettingsAuthState<jackin_config::EnvValue, AuthModal, PendingOpCommit>,
        SettingsTrustState,
        ErrorPopup,
    >;

#[must_use]
pub fn settings_save_preview<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    settings: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
) -> SettingsSavePreview {
    SettingsSavePreview {
        general: SettingsGeneralPreview {
            original_toggles: SettingsGeneralToggles {
                coauthor_trailer: settings.general.original_coauthor_trailer,
                dco: settings.general.original_dco,
            },
            pending_toggles: SettingsGeneralToggles {
                coauthor_trailer: settings.general.pending_coauthor_trailer,
                dco: settings.general.pending_dco,
            },
        },
        mounts_original: settings
            .mounts
            .original
            .iter()
            .map(global_mount_preview_row)
            .collect(),
        mounts_pending: settings
            .mounts
            .pending
            .iter()
            .map(global_mount_preview_row)
            .collect(),
        env_original: settings_env_preview(&settings.env.original),
        env_pending: settings_env_preview(&settings.env.pending),
        auth_original: settings.auth.original.clone(),
        auth_pending: settings.auth.pending.clone(),
        github_original: settings.auth.original_github.clone(),
        github_pending: settings.auth.github.clone(),
        bindings_original: settings.auth.original_bindings.clone(),
        bindings_pending: settings.auth.bindings.clone(),
        trust_original: settings
            .trust
            .original
            .iter()
            .map(|row| TrustPreviewRow {
                role: row.role.clone(),
                trusted: row.trusted,
            })
            .collect(),
        trust_pending: settings
            .trust
            .pending
            .iter()
            .map(|row| TrustPreviewRow {
                role: row.role.clone(),
                trusted: row.trusted,
            })
            .collect(),
    }
}

#[must_use]
pub fn build_settings_save_lines<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    settings: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
) -> Vec<Line<'static>> {
    settings_save_lines(&settings_save_preview(settings))
}

/// Toggle pair (git coauthor trailer + DCO enforcement) that the settings
/// dialog captures at edit time. Bundled so the parent `SettingsGeneralPreview`
/// keeps the `struct_excessive_bools` clippy gate quiet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsGeneralToggles {
    pub coauthor_trailer: bool,
    pub dco: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsGeneralPreview {
    pub original_toggles: SettingsGeneralToggles,
    pub pending_toggles: SettingsGeneralToggles,
}

impl SettingsGeneralPreview {
    pub(crate) fn change_count(self) -> usize {
        usize::from(self.original_toggles.coauthor_trailer != self.pending_toggles.coauthor_trailer)
            + usize::from(self.original_toggles.dco != self.pending_toggles.dco)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPreviewRow {
    pub scope: Option<String>,
    pub name: String,
    pub src: String,
    pub dst: String,
    pub readonly: bool,
}

#[must_use]
pub fn global_mount_preview_row(row: &jackin_config::GlobalMountRow) -> MountPreviewRow {
    MountPreviewRow {
        scope: row.scope.clone(),
        name: row.name.clone(),
        src: jackin_core::shorten_home(&row.mount.src),
        dst: jackin_core::shorten_home(&row.mount.dst),
        readonly: row.mount.readonly,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsEnvPreview {
    pub env: BTreeMap<String, String>,
    pub roles: BTreeMap<String, BTreeMap<String, String>>,
}

#[must_use]
pub fn settings_env_preview(
    config: &crate::tui::screens::settings::model::SettingsEnvConfig<jackin_config::EnvValue>,
) -> SettingsEnvPreview {
    SettingsEnvPreview {
        env: env_display_map_without_auth_credentials(&config.env),
        roles: config
            .roles
            .iter()
            .map(|(role, env)| (role.clone(), env_display_map_without_auth_credentials(env)))
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPreviewRow {
    pub role: String,
    pub trusted: bool,
}
