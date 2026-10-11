// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor tab, focus, and navigation plans.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Roles,
    Secrets,
    Auth,
}

impl EditorTab {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Mounts,
        Self::Roles,
        Self::Secrets,
        Self::Auth,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Mounts => "Mounts",
            Self::Roles => "Roles",
            Self::Secrets => "Environments",
            Self::Auth => "Accounts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFocusTarget {
    WorkspaceMounts,
    TabContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorHoverTarget {
    Tab(usize),
    MountRow(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleHeaderExpansionPlan {
    Set { role: String, expanded: bool },
    HeaderNoop,
    NotHeader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorRoleHeaderExpansionKeyPlan {
    Secrets(RoleHeaderExpansionPlan),
    NotRoleHeaderTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthEnterPlan {
    OpenForm,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEnterKeyPlan {
    OpenGeneralField,
    OpenMountFileBrowser,
    OpenSecretsPicker,
    OpenSecretsEnterModal,
    OpenRoleInput,
    Auth(AuthEnterPlan),
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEscapeKeyPlan {
    FocusTabBar,
    OpenSaveDiscard,
    ReloadFromConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSaveKeyPlan {
    BeginSave,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorMountGithubOpenPlan {
    NoSelection,
    NoGithubUrl,
    Open(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorHorizontalScrollKeyPlan {
    WorkspaceMounts { delta: i16, content_width: usize },
    TabContent { delta: i16, content_width: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorFieldSelectionKeyPlan {
    pub delta: isize,
    pub max_row: usize,
    pub skipped_rows: Vec<usize>,
    pub term: ratatui::layout::Rect,
    pub footer_h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorNavigationKeyPlan {
    MoveTab { delta: isize, focus_tab_bar: bool },
    FocusContent,
    FocusTabBar,
    NotNavigation,
}
