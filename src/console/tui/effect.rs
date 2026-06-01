//! Root-console TUI effect requests.

use jackin_console::tui::effect::ConsoleEffect;

use crate::console::tui::state::PendingSaveCommit;

#[derive(Debug)]
pub(crate) enum ManagerEffect {
    Console(ConsoleEffect),
    StartRoleRegistration {
        raw: String,
        key: String,
        selector: crate::selector::RoleSelector,
        source: crate::config::RoleSource,
    },
    ValidateOpCommit {
        op_ref: crate::operator_env::OpRef,
        is_settings: bool,
    },
}

pub(crate) enum WorkspaceSaveEffect {
    StartDriftCheck {
        original_name: String,
        prospective_mounts: Vec<crate::workspace::MountConfig>,
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    StartIsolationCleanup {
        records: Vec<crate::isolation::state::IsolationRecord>,
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
}

pub(crate) enum WorkspaceSaveWriteMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

pub(crate) struct WorkspaceSaveWriteInput<'a> {
    pub(crate) mode: WorkspaceSaveWriteMode,
    pub(crate) original: &'a crate::workspace::WorkspaceConfig,
    pub(crate) pending: &'a crate::workspace::WorkspaceConfig,
}

impl From<ConsoleEffect> for ManagerEffect {
    fn from(effect: ConsoleEffect) -> Self {
        Self::Console(effect)
    }
}
