// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `save_preview`.

use super::{
    MountPreviewRow, SettingsEnvPreview, SettingsGeneralPreview, SettingsGeneralToggles,
    SettingsSavePreview, TrustPreviewRow, WorkspaceAuthChange, WorkspaceMountDiff,
    WorkspaceMountPreviewRow, WorkspaceSaveMode, WorkspaceSavePreview, WorkspaceToggleSet,
    build_workspace_save_lines, settings_env_preview, settings_save_lines,
    workspace_create_display_name, workspace_save_lines,
};

use crate::mount_info_cache::MountInfoCache;

use crate::tui::screens::editor::model::EditorState;

use jackin_config::{
    AccountConfig, AccountCredential, AiProvider, AppConfig, EnvValue, WorkspaceConfig,
    WorkspaceRoleOverride,
};

use jackin_core::Agent;

use std::collections::BTreeMap;

mod support;
use support::*;
mod case_01;
