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
// the settings auth form after a mint. Pull from jackin_console directly to
// avoid routing through root's global_mounts thin shell (E0364 limitation
// when re-exporting items whose canonical path passes through a private mod).
pub(in crate::console) use jackin_console::tui::input::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
};

pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
    jackin_core::RoleSelector,
    jackin_core::Agent,
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
