//! Settings → Global Mounts tab input: thin adapter shell.
//!
//! All planning logic lives in `jackin-console`. This module only wires in the
//! root-owned source-folder validator (which calls a macOS `security`
//! subprocess and therefore cannot live in `jackin-console`).

use crossterm::event::KeyEvent;

pub(super) use jackin_console::tui::input::global_mounts::{
    SettingsAuthOutcome, SettingsModalOutcome, after_settings_event, handle_settings_confirm_modal,
    handle_settings_env_modal, handle_settings_key_with_effects,
};

/// Wrapper for root's call sites: supplies the concrete source-folder
/// validator so callers keep the 7-argument signature they already know.
pub(super) fn handle_settings_auth_modal(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    pending_token_generate: &mut Option<crate::console::tui::state::PendingTokenGenerate>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    term_size: ratatui::layout::Rect,
) -> SettingsAuthOutcome {
    jackin_console::tui::input::global_mounts::handle_settings_auth_modal(
        auth,
        env,
        pending_token_generate,
        key,
        op_available,
        op_cache,
        term_size,
        &crate::console::domain::validate_auth_source_folder,
    )
}
