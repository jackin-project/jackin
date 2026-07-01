//! Transitional root-console TUI facades.

pub(crate) mod input {
    //! Key dispatch for the workspace manager. Modal-first precedence:
    //! if a modal is open, events go to the modal handler; otherwise they
    //! go to the active stage's handler.

    pub(crate) mod editor {
        //! Thin adapter shell — editor-stage input dispatch lives in jackin-console.

        #[cfg(test)]
        pub(super) use jackin_console::tui::input::editor::{
            EditorModalOutcome, apply_file_browser_to_editor, apply_text_input_to_pending,
            env_key_input_state, handle_editor_modal,
        };
        #[cfg(test)]
        pub(super) use jackin_console::tui::screens::editor::view::{
            role_load_input_state, secret_new_key_label,
        };

        #[cfg(test)]
        pub(super) fn poll_role_load(
            editor: &mut crate::console::tui::state::EditorState<'_>,
            config: &mut jackin_config::AppConfig,
            paths: &jackin_core::JackinPaths,
        ) -> bool {
            use crate::console::tui::state::PendingRoleLoad;
            use jackin_console::tui::model::ConsolePendingRoleLoad as _;
            let Some((load, result)): Option<(PendingRoleLoad, anyhow::Result<()>)> =
                editor.poll_pending_role_load()
            else {
                return false;
            };
            crate::console::effects::apply_role_load_completion_for_tests(
                editor, config, paths, load, result,
            );
            true
        }

        #[cfg(test)]
        pub(super) mod tests;
    }

    mod dispatch {
        //! Thin root adapter: binds jackin-console's generic key dispatcher to
        //! the root-binary `validate_auth_source_folder` implementation.

        use super::InputOutcome;
        use crate::console::tui::state::ManagerState;
        use jackin_config::AppConfig;
        use jackin_core::JackinPaths;

        pub fn handle_key(
            state: &mut ManagerState<'_>,
            config: &mut AppConfig,
            paths: &JackinPaths,
            cwd: &std::path::Path,
            key: crossterm::event::KeyEvent,
        ) -> anyhow::Result<InputOutcome> {
            jackin_console::tui::input::dispatch::handle_key(
                state,
                config,
                paths,
                cwd,
                key,
                &crate::console::validate_auth_source_folder,
            )
        }

        #[cfg(test)]
        mod tests;
    }

    pub use dispatch::handle_key;
    pub(crate) use jackin_console::tui::input::mouse::{clickable_at, handle_mouse_with_config};

    pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
        jackin_core::RoleSelector,
        jackin_core::Agent,
        crate::console::ConsoleInstanceAction,
        jackin_protocol::Provider,
    >;

    #[cfg(test)]
    pub(super) mod test_support;
}
pub mod run;
pub mod state {
    //! Manager state machine for the jackin❯ console TUI.
    //!
    //! `ManagerState` and all concrete type aliases now live in `jackin-console`.
    //! This module re-exports the full public surface.

    pub use jackin_console::tui::state::*;

    // These re-imports are used by the child `tests` module via `use super::*`.
    // Child modules have access to private items of their parent, so placing them
    // here (even without `pub`) makes them available to tests without polluting
    // the crate's public API.
    #[cfg(test)]
    use jackin_config::AppConfig;
    #[cfg(test)]
    use jackin_console::tui::auth::AuthKind;
    #[cfg(test)]
    use jackin_core::EnvValue;

    #[cfg(test)]
    mod tests;
}

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

pub use input::{InputOutcome, handle_key};
pub use jackin_console::tui::console::{
    ConsoleStage, ConsoleState, new_console_state, new_console_state_with_op_available,
};
pub use jackin_console::tui::launch::dispatch_launch_for_workspace;
pub(crate) use jackin_console::tui::state::update::{ManagerMessage, update_manager};
pub use jackin_console::tui::view::{prepare_for_render, render};
pub use run::run_console;
pub use state::{ManagerStage, ManagerState};
