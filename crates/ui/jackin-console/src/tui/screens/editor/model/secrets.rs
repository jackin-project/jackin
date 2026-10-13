// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor secret and auth row types.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Row(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Role(String),
}

/// Flat row model for the Secrets tab; cursor is a single index.
#[derive(Debug, Clone)]
pub enum SecretsRow {
    WorkspaceKeyRow(String),
    WorkspaceAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleKeyRow {
        role: String,
        key: String,
    },
    RoleAddSentinel(String),
    /// Non-focusable; cursor Up/Down skips over it.
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

/// Row-shape model for the Auth tab.
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
