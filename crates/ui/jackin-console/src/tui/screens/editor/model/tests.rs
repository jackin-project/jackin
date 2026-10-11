// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::EditorErrorPopupModal;
use super::EditorHoverTarget;
use super::EditorRoleOverridePickerModal;
use super::EditorSaveDiscardModal;
#[cfg(test)]
use super::SecretsEnterPlan;
use super::SecretsScopeTag;
use jackin_config::{
    MountConfig, MountIsolation, RoleSource, WorkspaceConfig, WorkspaceRoleOverride,
};

use super::{
    AuthEnterPlan, AuthRow, EditorAuthActionKeyPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan,
    EditorFieldSelectionKeyPlan, EditorFocusTarget, EditorHorizontalScrollKeyPlan,
    EditorImmediateActionKeyPlan, EditorMode, EditorMountActionKeyPlan, EditorMountGithubOpenPlan,
    EditorNavigationKeyPlan, EditorRoleActionKeyPlan, EditorSaveKeyPlan, EditorSaveModePlan,
    EditorSecretsActionKeyPlan, EditorState, EditorStatusPopupModal, EditorTab,
    EditorTabActionKeyPlan, FieldFocus, RoleHeaderExpansionPlan, SecretsRow, editor_save_mode_plan,
};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
