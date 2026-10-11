// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::SettingsSaveInput;
use super::save_settings;
use std::collections::BTreeMap;

use jackin_config::{
    AppConfig, EnvScope, EnvValue, GithubAuthConfig, KeepAwakeConfig, MountConfig, MountIsolation,
    WorkspaceConfig, WorkspaceRoleOverride,
};

use jackin_core::{Agent, WorkspaceName};

use super::{
    EditorSavePreviewError, EditorSavePreviewInput, EditorSavePreviewPlan, WorkspaceSaveDiffOp,
    build_workspace_edit, plan_editor_save_preview, pre_existing_redundant_mounts_message,
    workspace_save_diff_plan,
};

use crate::services::config_save::validate_settings_env;

use crate::tui::screens::settings::model::{SettingsEnvConfig, SettingsTrustRow};

mod support;
use support::*;
mod case_01;
mod case_02;
