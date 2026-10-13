// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor action and save-mode plans.

use super::{EditorEnterKeyPlan, EditorNavigationKeyPlan, SecretsScopeTag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTopLevelKeyPlan {
    Save,
    Escape,
    Navigation(EditorNavigationKeyPlan),
    ScrollHorizontal { delta: i16 },
    MoveField { delta: isize },
    SetRoleHeaderExpanded { expanded: bool },
    CheckImmediateAction,
    ContinueToTabActions,
}

// Editor top-level key dispatch lives in `input/editor.rs`
// (`dispatch_editor_top_level`), which resolves keys through the
// `EDITOR_GLOBAL` / `EDITOR_TAB_BAR` / `EDITOR_CONTENT` keymaps in
// `tui::keymap`. There is deliberately no parallel `match` encoding here: the
// keymap registry is the single source of truth, and its precedence is covered
// by `input::editor::tests::dispatch_editor_top_level_preserves_precedence`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorImmediateActionKeyPlan {
    ToggleGeneralSelected,
    ToggleMountReadonlySelected,
    ToggleSecretMask { scope: SecretsScopeTag, key: String },
    NotImmediateAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorRoleActionKeyPlan {
    OpenRoleInput,
    ToggleAllowed,
    ToggleDefault,
    NotRoleAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMountActionKeyPlan {
    AddMount,
    RemoveSelectedMount,
    CycleIsolation,
    OpenGithub,
    NotMountAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSecretsActionKeyPlan {
    OpenPicker,
    OpenDeleteConfirm,
    OpenAddModal,
    NotSecretsAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthActionKeyPlan {
    ClearFocusedRow,
    NotAuthAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorTabActionKeyPlan {
    Role(EditorRoleActionKeyPlan),
    Mount(EditorMountActionKeyPlan),
    Secrets(EditorSecretsActionKeyPlan),
    Auth(EditorAuthActionKeyPlan),
    Enter(EditorEnterKeyPlan),
    Noop,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSaveModePlan {
    Edit { original_name: String },
    Create,
}

#[must_use]
pub fn editor_save_mode_plan(mode: &EditorMode) -> EditorSaveModePlan {
    match mode {
        EditorMode::Edit { name } => EditorSaveModePlan::Edit {
            original_name: name.clone(),
        },
        EditorMode::Create => EditorSaveModePlan::Create,
    }
}
