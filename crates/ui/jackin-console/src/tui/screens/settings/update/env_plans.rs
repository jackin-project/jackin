// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings env picker and commit plans.

use super::RolePickerOpenPlan;

use super::super::model::{SettingsEnvConfig, SettingsEnvScope, SettingsEnvTextTarget};

use jackin_core::RoleSelector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvTextCommitPlan {
    EmptyKey {
        scope: SettingsEnvScope,
    },
    SetCarriedPickerValue {
        scope: SettingsEnvScope,
        key: String,
    },
    OpenSourcePicker {
        scope: SettingsEnvScope,
        key: String,
    },
    SetPlainValue {
        scope: SettingsEnvScope,
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvSourcePickerSelection {
    Plain,
    Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvSourcePickerCommitPlan {
    OpenPlainText {
        scope: SettingsEnvScope,
        key: String,
    },
    OpenOpPicker {
        scope: SettingsEnvScope,
        key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvScopePickerSelection {
    AllAgents,
    SpecificAgent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvScopePickerCommitPlan {
    OpenGlobalKeyInput { scope: SettingsEnvScope },
    OpenRolePicker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvRolePickerCommitPlan {
    pub scope: SettingsEnvScope,
}

#[must_use]
pub fn settings_env_text_commit_plan(
    target: &SettingsEnvTextTarget,
    value: &str,
    has_carried_picker_value: bool,
) -> SettingsEnvTextCommitPlan {
    match target {
        SettingsEnvTextTarget::EnvKey { scope } => {
            let key = value.trim();
            if key.is_empty() {
                return SettingsEnvTextCommitPlan::EmptyKey {
                    scope: scope.clone(),
                };
            }
            if has_carried_picker_value {
                SettingsEnvTextCommitPlan::SetCarriedPickerValue {
                    scope: scope.clone(),
                    key: key.to_owned(),
                }
            } else {
                SettingsEnvTextCommitPlan::OpenSourcePicker {
                    scope: scope.clone(),
                    key: key.to_owned(),
                }
            }
        }
        SettingsEnvTextTarget::EnvValue { scope, key } => {
            SettingsEnvTextCommitPlan::SetPlainValue {
                scope: scope.clone(),
                key: key.clone(),
                value: value.to_owned(),
            }
        }
    }
}

#[must_use]
pub fn settings_env_source_picker_commit_plan(
    selection: SettingsEnvSourcePickerSelection,
    source_key: &(SettingsEnvScope, String),
) -> SettingsEnvSourcePickerCommitPlan {
    let (scope, key) = source_key;
    match selection {
        SettingsEnvSourcePickerSelection::Plain => {
            SettingsEnvSourcePickerCommitPlan::OpenPlainText {
                scope: scope.clone(),
                key: key.clone(),
            }
        }
        SettingsEnvSourcePickerSelection::Op => SettingsEnvSourcePickerCommitPlan::OpenOpPicker {
            scope: scope.clone(),
            key: key.clone(),
        },
    }
}

#[must_use]
pub const fn settings_env_scope_picker_commit_plan(
    selection: SettingsEnvScopePickerSelection,
) -> SettingsEnvScopePickerCommitPlan {
    match selection {
        SettingsEnvScopePickerSelection::AllAgents => {
            SettingsEnvScopePickerCommitPlan::OpenGlobalKeyInput {
                scope: SettingsEnvScope::Global,
            }
        }
        SettingsEnvScopePickerSelection::SpecificAgent => {
            SettingsEnvScopePickerCommitPlan::OpenRolePicker
        }
    }
}

#[must_use]
pub fn settings_env_role_picker_commit_plan(
    role: &RoleSelector,
) -> SettingsEnvRolePickerCommitPlan {
    SettingsEnvRolePickerCommitPlan {
        scope: SettingsEnvScope::Role(role.key()),
    }
}

#[must_use]
pub fn settings_env_role_picker_roles<V>(pending: &SettingsEnvConfig<V>) -> Vec<RoleSelector> {
    pending
        .roles
        .keys()
        .filter_map(|role| RoleSelector::parse(role).ok())
        .collect()
}

#[must_use]
pub fn settings_env_role_picker_open_plan<V>(pending: &SettingsEnvConfig<V>) -> RolePickerOpenPlan {
    role_picker_open_plan(settings_env_role_picker_roles(pending))
}

#[must_use]
pub fn role_picker_open_plan(roles: Vec<RoleSelector>) -> RolePickerOpenPlan {
    if roles.is_empty() {
        RolePickerOpenPlan::NoRoles
    } else {
        RolePickerOpenPlan::Open(roles)
    }
}
