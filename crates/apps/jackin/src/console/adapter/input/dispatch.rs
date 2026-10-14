// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::InputOutcome;
use crate::console::adapter::state::ManagerState;
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
