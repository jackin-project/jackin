//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

pub(crate) mod auth;
mod dispatch;
pub(crate) mod editor;
pub(crate) mod global_mounts;
pub(crate) mod list;
pub(crate) mod mouse;
pub(crate) mod prelude;
pub(crate) mod save;

pub use dispatch::handle_key;
pub(crate) use mouse::{clickable_at, handle_mouse_with_config};

// Re-exported for the `run_console` token-generate loop, which re-mounts
// the settings auth form after a mint (the `global_mounts` module is
// `pub(super)`, so the loop reaches the helpers through this seam).
pub(in crate::console) use global_mounts::{
    apply_op_picker_settings_commit_failed, apply_op_picker_to_settings_auth_form_committed,
    apply_plain_text_to_settings_auth_form,
};

pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
    crate::selector::RoleSelector,
    crate::agent::Agent,
    crate::console::ConsoleInstanceAction,
    jackin_protocol::Provider,
>;

/// Cross-submodule helpers for the input/* test modules. Lifted out of
/// the per-submodule test blocks because `key()` and `mount()` show up in
/// virtually every test file; keeping a single canonical definition
/// avoids the previous problem where each submodule grew its own
/// near-identical copy.
#[cfg(test)]
pub(super) mod test_support {
    use crate::workspace::MountConfig;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }
}

#[cfg(test)]
mod dispatch_tests;
