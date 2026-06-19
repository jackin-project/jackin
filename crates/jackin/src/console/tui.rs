//! Transitional root-console TUI facades.

pub(crate) mod input {
    //! Key dispatch for the workspace manager. Modal-first precedence:
    //! if a modal is open, events go to the modal handler; otherwise they
    //! go to the active stage's handler.

    mod dispatch;
    pub(crate) mod editor;

    pub use dispatch::handle_key;
    pub(crate) use jackin_console::tui::input::mouse::{clickable_at, handle_mouse_with_config};

    pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
        jackin_core::RoleSelector,
        jackin_core::Agent,
        crate::console::ConsoleInstanceAction,
        jackin_protocol::Provider,
    >;

    #[cfg(test)]
    pub(super) mod test_support {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        use jackin_config::{MountConfig, MountIsolation};

        pub(crate) fn key(code: KeyCode) -> KeyEvent {
            KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }
        }

        pub(crate) fn mount(src: &str, dst: &str) -> MountConfig {
            MountConfig {
                src: src.into(),
                dst: dst.into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }
        }
    }
}
pub mod run;
pub mod state;

// ── Root-console type aliases ───────────────────────────────────────────────
// Concrete manager effect and save-flow types binding generic jackin-console
// effect vocabulary to root-binary-owned types.

pub(crate) type ManagerEffect = jackin_console::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

pub(crate) type WorkspaceSaveEffect = jackin_console::tui::effect::WorkspaceSaveEffect<
    jackin_config::MountConfig,
    state::PendingSaveCommit,
    jackin_runtime::isolation::state::IsolationRecord,
    jackin_config::WorkspaceConfig,
>;

pub(crate) type WorkspaceSaveWriteMode = jackin_console::tui::effect::WorkspaceSaveWriteMode;

pub(crate) type WorkspaceSaveWriteInput<'a> =
    jackin_console::tui::effect::WorkspaceSaveWriteInput<'a, jackin_config::WorkspaceConfig>;

pub use jackin_console::tui::console::{
    ConsoleStage, ConsoleState, new_console_state, new_console_state_with_op_available,
};
pub use input::{InputOutcome, handle_key};
pub use jackin_console::tui::launch::dispatch_launch_for_workspace;
pub use jackin_console::tui::view::{prepare_for_render, render};
pub(crate) use jackin_console::tui::state::update::{ManagerMessage, update_manager};
pub use run::run_console;
pub use state::{ManagerStage, ManagerState};
