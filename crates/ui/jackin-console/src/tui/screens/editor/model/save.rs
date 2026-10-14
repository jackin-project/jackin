// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::SecretsScopeTag;

#[derive(Debug, Clone)]
pub struct PendingSaveCommit<M> {
    pub effective_removals: Vec<String>,
    pub final_mounts: Option<Vec<M>>,

    pub delete_isolated_acknowledged: bool,

    pub isolated_cleanup_complete: bool,
}

#[derive(Debug, Clone, Default)]
pub enum EditorSaveFlow<P> {
    #[default]
    Idle,
    Confirming {
        exit_on_success: bool,
    },
    PendingCommit {
        plan: P,
        exit_on_success: bool,
    },
    Error {
        message: String,
    },
}

impl<P> EditorSaveFlow<P> {
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    #[must_use]
    pub const fn error_message(&self) -> Option<&str> {
        if let Self::Error { message } = self {
            Some(message.as_str())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfirmTarget<R, P> {
    DeleteEnvVar {
        scope: SecretsScopeTag,
        key: String,
    },
    TrustRoleSource {
        key: String,
        source: R,
    },
    DeleteIsolatedAndSave {
        plan: P,
        exit_on_success: bool,
        affected_containers: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
    Role,
    EnvKey {
        scope: SecretsScopeTag,
    },
    EnvKeyWithValue {
        scope: SecretsScopeTag,
        value: jackin_core::EnvValue,
    },
    EnvValue {
        scope: SecretsScopeTag,
        key: String,
    },
    AuthCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
    AuthFormSourceFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
}
