use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Mounts,
    Environments,
    Auth,
    Trust,
}

impl SettingsTab {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Mounts,
        Self::Environments,
        Self::Auth,
        Self::Trust,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Mounts => "Mounts",
            Self::Environments => "Environments",
            Self::Auth => "Auth",
            Self::Trust => "Trust",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn previous(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Roles,
    Secrets,
    Auth,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Row(usize),
}

/// Cursor position inside the auth-edit form modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormFocus {
    Mode,
    CredentialSource,
    Save,
    Cancel,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Role(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsEnvScope {
    Global,
    Role(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountConfirm {
    Remove,
    Save,
    Sensitive,
    Discard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountTextTarget {
    AddScope,
    AddName,
    AddSource,
    AddDestination,
    Source,
    Destination,
    Scope,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
    Role,
    EnvKey { scope: SecretsScopeTag },
    EnvValue { scope: SecretsScopeTag, key: String },
    AuthCredential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsTrustRow {
    pub role: String,
    pub git: String,
    pub trusted: bool,
}

#[derive(Debug, Default)]
pub struct GlobalMountDraft {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SettingsGeneralState {
    pub pending_coauthor_trailer: bool,
    pub original_coauthor_trailer: bool,
    pub pending_dco: bool,
    pub original_dco: bool,
    pub selected: usize,
}

impl SettingsGeneralState {
    #[must_use]
    pub const fn from_values(coauthor_trailer: bool, dco: bool) -> Self {
        Self {
            pending_coauthor_trailer: coauthor_trailer,
            original_coauthor_trailer: coauthor_trailer,
            pending_dco: dco,
            original_dco: dco,
            selected: 0,
        }
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.pending_coauthor_trailer != self.original_coauthor_trailer
            || self.pending_dco != self.original_dco
    }

    pub const fn discard(&mut self) {
        self.pending_coauthor_trailer = self.original_coauthor_trailer;
        self.pending_dco = self.original_dco;
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        usize::from(self.pending_coauthor_trailer != self.original_coauthor_trailer)
            + usize::from(self.pending_dco != self.original_dco)
    }

    pub const fn mark_clean(&mut self) {
        self.original_coauthor_trailer = self.pending_coauthor_trailer;
        self.original_dco = self.pending_dco;
    }
}

#[must_use]
pub fn settings_vec_change_count<T: PartialEq>(original: &[T], pending: &[T]) -> usize {
    let common_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a != b)
        .count();
    common_changes + original.len().abs_diff(pending.len())
}

#[must_use]
pub fn settings_map_change_count<V: PartialEq>(
    original: &BTreeMap<String, V>,
    pending: &BTreeMap<String, V>,
) -> usize {
    let mut count = 0;
    for (key, value) in pending {
        match original.get(key) {
            None => count += 1,
            Some(original_value) if original_value != value => count += 1,
            _ => {}
        }
    }
    for key in original.keys() {
        if !pending.contains_key(key) {
            count += 1;
        }
    }
    count
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    PickFirstMountSrc,
    PickFirstMountDst,
    PickWorkdir,
    NameWorkspace,
}
