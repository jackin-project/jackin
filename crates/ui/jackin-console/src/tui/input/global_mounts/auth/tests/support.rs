// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn modal_key(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    code: KeyCode,
) -> SettingsAuthOutcome {
    handle_settings_auth_modal(
        auth,
        env,
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
        false,
        std::rc::Rc::new(std::cell::RefCell::new(jackin_env::OpCache::default())),
        ratatui::layout::Rect::new(0, 0, 100, 40),
        &|_, _| Ok(()),
    )
}

pub(super) fn state() -> (
    crate::tui::state::SettingsAuthState,
    crate::tui::state::SettingsEnvState<'static>,
) {
    let config = jackin_config::AppConfig::default();
    (
        crate::tui::state::SettingsAuthState::from_config(&config),
        crate::tui::state::SettingsEnvState::from_config(&config),
    )
}
