use std::collections::BTreeMap;

use ratatui::text::Line;

use crate::tui::auth_config::env_display_map;
use crate::tui::screens::settings::model::{
    GlobalMountsState, SettingsAuthState, SettingsEnvState, SettingsState, SettingsTrustState,
};

use super::settings_lines::settings_save_lines;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSavePreview {
    pub general: SettingsGeneralPreview,
    pub mounts_original: Vec<MountPreviewRow>,
    pub mounts_pending: Vec<MountPreviewRow>,
    pub env_original: SettingsEnvPreview,
    pub env_pending: SettingsEnvPreview,
    pub auth_original: Vec<AuthPreviewRow>,
    pub auth_pending: Vec<AuthPreviewRow>,
    pub auth_github_env_original: BTreeMap<String, String>,
    pub auth_github_env_pending: BTreeMap<String, String>,
    pub trust_original: Vec<TrustPreviewRow>,
    pub trust_pending: Vec<TrustPreviewRow>,
}

pub type ConsoleSettingsState<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
> = SettingsState<
    GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
    SettingsEnvState<jackin_config::EnvValue, EnvModal>,
    SettingsAuthState<jackin_config::EnvValue, AuthModal, PendingOpCommit>,
    SettingsTrustState,
    ErrorPopup,
    PendingToken,
>;

#[must_use]
pub fn settings_save_preview<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    settings: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
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
        auth_original: settings
            .auth
            .original
            .iter()
            .map(|row| AuthPreviewRow {
                label: row.kind.label().to_owned(),
                mode: row.mode.as_str().to_owned(),
            })
            .collect(),
        auth_pending: settings
            .auth
            .pending
            .iter()
            .map(|row| AuthPreviewRow {
                label: row.kind.label().to_owned(),
                mode: row.mode.as_str().to_owned(),
            })
            .collect(),
        auth_github_env_original: env_display_map(&settings.auth.original_github_env),
        auth_github_env_pending: env_display_map(&settings.auth.github_env),
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
pub fn build_settings_save_lines<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    settings: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
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
    pub(super) fn change_count(self) -> usize {
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
        src: jackin_tui::shorten_home(&row.mount.src),
        dst: jackin_tui::shorten_home(&row.mount.dst),
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
        env: env_display_map(&config.env),
        roles: config
            .roles
            .iter()
            .map(|(role, env)| (role.clone(), env_display_map(env)))
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPreviewRow {
    pub label: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPreviewRow {
    pub role: String,
    pub trusted: bool,
}
