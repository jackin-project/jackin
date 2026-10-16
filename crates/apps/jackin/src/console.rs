// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    irrefutable_let_patterns,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]

pub mod effects;
mod services;
pub(crate) mod workflows;
pub mod terminal {

    pub use jackin_console::tui::terminal::TerminalSession;
    pub(crate) use jackin_console::tui::terminal::{
        MAX_EVENTS_PER_TICK, MOUSE_ESCAPE_GRACE_MS, TICK_MS,
    };

    struct HostConsoleTerminal;

    impl jackin_console::ConsoleHostTerminal for HostConsoleTerminal {
        fn begin_debug_buffering(&self) {
            jackin_diagnostics::begin_debug_buffering();
        }

        fn end_debug_buffering(&self) {
            jackin_diagnostics::end_debug_buffering();
        }

        fn set_host_screen_owned(&self, owned: bool) {
            crate::terminal_ownership::set_host_screen_owned(owned);
        }

        fn host_screen_owned(&self) -> bool {
            crate::terminal_ownership::host_screen_owned()
        }
    }

    static HOST_CONSOLE_TERMINAL: HostConsoleTerminal = HostConsoleTerminal;

    pub(crate) fn host_console_terminal() -> &'static dyn jackin_console::ConsoleHostTerminal {
        &HOST_CONSOLE_TERMINAL
    }
}
pub mod adapter;

pub(super) fn validate_auth_source_folder(
    kind: Option<jackin_console::tui::auth::AuthKind>,
    path: &std::path::Path,
) -> Result<(), String> {
    use jackin_console::tui::auth_config::auth_kind_agent;
    let Some(agent) = kind.and_then(auth_kind_agent) else {
        return Ok(());
    };
    let host_home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    jackin_runtime::instance::validate_sync_source_dir(agent, path, &host_home)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) use jackin_console::services::role_source::resolve_role_input_source;

pub use adapter::{ConsoleStage, ConsoleState, run_console};
pub use jackin_console::services::launch::{WorkspaceChoice, build_workspace_choice};
pub use terminal::TerminalSession;

pub type ConsoleInstanceAction =
    jackin_console::tui::message::ConsoleInstanceAction<jackin_core::Agent>;
pub type ConsoleOutcome = jackin_console::tui::message::ConsoleOutcome<
    jackin_core::RoleSelector,
    jackin_config::ResolvedWorkspace,
    jackin_core::Agent,
    jackin_core::LaunchSelection,
>;
pub use jackin_console::tui::message::InstanceActionHandler;
