// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Row(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Role(String),
}

#[derive(Debug, Clone)]
pub enum SecretsRow {
    WorkspaceKeyRow(String),
    WorkspaceAddSentinel,
    RoleHeader { role: String, expanded: bool },
    RoleKeyRow { role: String, key: String },
    RoleAddSentinel(String),

    SectionSpacer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsEnterPlan {
    EditValue { scope: SecretsScopeTag, key: String },
    OpenScopePicker,
    ExpandRole(String),
    AddRoleKey { scope: SecretsScopeTag },
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRow<K> {
    Account {
        id: String,
    },
    Binding {
        agent: jackin_core::Agent,
        role: Option<String>,
    },
    WorkspaceMode {
        kind: K,
    },
    RoleMode {
        role: String,
        kind: K,
    },
}
