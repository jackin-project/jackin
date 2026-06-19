//! Thin root adapter: binds jackin-console's generic key dispatcher to
//! the root-binary validate_auth_source_folder implementation.

use super::InputOutcome;
use crate::console::tui::state::ManagerState;
use crate::paths::JackinPaths;
use jackin_config::AppConfig;

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
