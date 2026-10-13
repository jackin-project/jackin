// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings env rows and config.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsEnvScope {
    Global,
    Role(String),
}

#[derive(Debug, Clone)]
pub enum SettingsEnvRow {
    Key {
        scope: SettingsEnvScope,
        key: String,
    },
    GlobalAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleAddSentinel(String),
    SectionSpacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvConfirm {
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvTextTarget {
    EnvKey {
        scope: SettingsEnvScope,
    },
    EnvValue {
        scope: SettingsEnvScope,
        key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvEnterPlan {
    EditValue {
        scope: SettingsEnvScope,
        key: String,
    },
    OpenScopePicker,
    ExpandRole(String),
    AddRoleKey {
        scope: SettingsEnvScope,
    },
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvOpPickerTarget {
    Existing {
        scope: SettingsEnvScope,
        key: String,
    },
    NewKey {
        scope: SettingsEnvScope,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvConfig<V> {
    pub env: BTreeMap<String, V>,
    pub roles: BTreeMap<String, BTreeMap<String, V>>,
}

impl<V> SettingsEnvConfig<V> {
    pub fn map<U>(self, mut f: impl FnMut(V) -> U) -> SettingsEnvConfig<U> {
        SettingsEnvConfig {
            env: self
                .env
                .into_iter()
                .map(|(key, value)| (key, f(value)))
                .collect(),
            roles: self
                .roles
                .into_iter()
                .map(|(role, env)| {
                    (
                        role,
                        env.into_iter()
                            .map(|(key, value)| (key, f(value)))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}
