// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn confirm_modal(
    settings: &mut SettingsState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) {
    let outcome = handle_settings_confirm_modal(settings, key, Rect::new(0, 0, 120, 40));
    if matches!(outcome, SettingsModalOutcome::SaveSettings) {
        match crate::services::config_save::save_settings(
            paths,
            crate::services::config_save::SettingsSaveInput {
                mounts_original: &settings.mounts.original,
                mounts_pending: &settings.mounts.pending,
                env_original: &settings.env.original,
                env_pending: &settings.env.pending,
                auth_pending: &settings.auth.pending,
                auth_original: &settings.auth.original,
                original_github: &settings.auth.original_github,
                bindings_pending: &settings.auth.bindings,
                bindings_original: &settings.auth.original_bindings,
                github: &settings.auth.github,
                trust_pending: &settings.trust.pending,
                git_coauthor_trailer: settings.general.pending_coauthor_trailer,
                git_dco: settings.general.pending_dco,
            },
        ) {
            Ok(saved) => {
                *config = saved;
                settings.mark_saved();
                settings.mounts.exit_requested = true;
            }
            Err(err) => settings.mounts.error = Some(err.to_string()),
        }
    }
    if matches!(outcome, SettingsModalOutcome::OpenGlobalMountFileBrowser) {
        match crate::services::file_browser::state_from_home() {
            Ok(file_browser) => {
                settings
                    .mounts
                    .open_sub_modal(SettingsModal::MountFileBrowser {
                        state: Box::new(file_browser),
                    });
            }
            Err(error) => {
                settings.mounts.add_draft = None;
                settings.mounts.error = Some(error.to_string());
            }
        }
    }
    assert!(
        !matches!(outcome, SettingsModalOutcome::OpenUrl(_)),
        "test helper did not expect URL-open"
    );
}
