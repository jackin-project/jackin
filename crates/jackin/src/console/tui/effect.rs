//! Root-console aliases for crate-owned TUI effect requests.

pub(crate) type ManagerEffect = jackin_console::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    crate::config::RoleSource,
    crate::operator_env::OpRef,
>;

pub(crate) type FileBrowserEffectContext = jackin_console::tui::effect::FileBrowserEffectContext;

pub(crate) type WorkspaceSaveEffect = jackin_console::tui::effect::WorkspaceSaveEffect<
    crate::workspace::MountConfig,
    crate::console::tui::state::PendingSaveCommit,
    crate::isolation::state::IsolationRecord,
    crate::workspace::WorkspaceConfig,
>;

pub(crate) type WorkspaceSaveWriteMode = jackin_console::tui::effect::WorkspaceSaveWriteMode;

pub(crate) type WorkspaceSaveWriteInput<'a> =
    jackin_console::tui::effect::WorkspaceSaveWriteInput<'a, crate::workspace::WorkspaceConfig>;
