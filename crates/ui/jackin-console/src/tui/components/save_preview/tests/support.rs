// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) type TestEditorState = EditorState<MountInfoCache, (), (), EnvValue, (), (), (), ()>;

pub(super) fn empty_workspace_preview() -> WorkspaceSavePreview {
    WorkspaceSavePreview {
        mode: WorkspaceSaveMode::Edit {
            original_name: "demo".to_owned(),
            display_name: "demo".to_owned(),
            pending_name: None,
        },
        original_workdir: Some("/repo".to_owned()),
        pending_workdir: "/repo".to_owned(),
        mount_diffs: Vec::new(),
        auth_changes: Vec::new(),
        original_allowed_roles: Vec::new(),
        pending_allowed_roles: Vec::new(),
        role_count: 0,
        original_default_role: None,
        pending_default_role: None,
        original_toggles: WorkspaceToggleSet::default(),
        pending_toggles: WorkspaceToggleSet::default(),
        env_original: SettingsEnvPreview::default(),
        env_pending: SettingsEnvPreview::default(),
        collapse_lines: Vec::new(),
    }
}

pub(super) fn empty_settings_preview() -> SettingsSavePreview {
    SettingsSavePreview {
        general: SettingsGeneralPreview {
            original_toggles: SettingsGeneralToggles::default(),
            pending_toggles: SettingsGeneralToggles::default(),
        },
        mounts_original: Vec::new(),
        mounts_pending: Vec::new(),
        env_original: SettingsEnvPreview::default(),
        env_pending: SettingsEnvPreview::default(),
        auth_original: BTreeMap::new(),
        auth_pending: BTreeMap::new(),
        github_original: jackin_config::GithubAuthConfig::default(),
        github_pending: jackin_config::GithubAuthConfig::default(),
        bindings_original: BTreeMap::new(),
        bindings_pending: BTreeMap::new(),
        trust_original: Vec::new(),
        trust_pending: Vec::new(),
    }
}

pub(super) fn line_text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn edit_lines(original: WorkspaceConfig, pending: WorkspaceConfig) -> String {
    let config = AppConfig::default();
    let mut editor = TestEditorState::new_edit("demo".to_owned(), original);
    editor.pending = pending;
    line_text(&build_workspace_save_lines(&editor, &config, &[]))
}

pub(super) fn account(secret: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: EnvValue::Plain(secret.into()),
            base_url: None,
            model: None,
        },
    }
}
