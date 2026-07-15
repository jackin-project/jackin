// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Binary adapter for the jackin❯ interactive console (`jackin console`).
//!
//! Phase 5 adapter-shell end state. Entry point is `run_console`. All
//! console product state, update, view, input planning, and business rules
//! live in `jackin-console` or lower crates. This module only binds that
//! surface to the binary crate's concrete config, Docker, runtime, terminal,
//! and CLI services.
//!
//! * `terminal` — inline host-terminal adapter (debug-buffering globals).
//! * `tui` — inline type aliases + thin input/state adapters; `tui/run.rs` owns the event loop.
//! * `services` — root IO adapters (config, instances, role load, workspace save).
//! * `effects` — interpreter for typed `ManagerEffect` values.

// `ConsoleStage` collapsed to a single variant in PR #171's Modal::RolePicker
// cleanup. The module is kept as-is (with `if let ConsoleStage::Manager(_)`
// patterns) so a future stage can be added without rewriting every match
// site. The irrefutable-pattern lint is allowed at the module level rather
// than peppering individual sites.
#![expect(
    irrefutable_let_patterns,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]

pub mod effects;
mod services;
pub mod terminal {
    //! Host adapter for console terminal ownership.
    //!
    //! Terminal lifecycle lives in `jackin-console`'s TUI boundary. This root
    //! module only binds that generic terminal code to the root crate's host
    //! debug-buffering globals.

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
            jackin_tui::ownership::set_host_screen_owned(owned);
        }

        fn host_screen_owned(&self) -> bool {
            jackin_tui::ownership::host_screen_owned()
        }
    }

    static HOST_CONSOLE_TERMINAL: HostConsoleTerminal = HostConsoleTerminal;

    pub(crate) fn host_console_terminal() -> &'static dyn jackin_console::ConsoleHostTerminal {
        &HOST_CONSOLE_TERMINAL
    }
}
pub mod tui;

/// Validate a picked source folder against the agent an auth form targets.
/// Returns `Ok(())` for non-agent auth kinds. Runtime validation stays in the
/// binary adapter because `jackin-console` cannot depend on runtime.
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

pub use jackin_console::services::launch::{WorkspaceChoice, build_workspace_choice};
pub use terminal::TerminalSession;
pub use tui::{ConsoleStage, ConsoleState, run_console};

pub type ConsoleInstanceAction =
    jackin_console::tui::message::ConsoleInstanceAction<jackin_core::Agent>;
pub type ConsoleOutcome = jackin_console::tui::message::ConsoleOutcome<
    jackin_core::RoleSelector,
    jackin_config::ResolvedWorkspace,
    jackin_core::Agent,
    jackin_protocol::Provider,
>;
pub use jackin_console::tui::message::InstanceActionHandler;
