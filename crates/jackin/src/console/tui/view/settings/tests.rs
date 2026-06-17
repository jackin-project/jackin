//! Tests for `settings`.
use super::*;
use crate::config::AppConfig;
use crate::console::tui::state::SettingsState;
use ratatui::{Terminal, backend::TestBackend};

fn render_settings_to_dump(state: &SettingsState<'_>) -> String {
    let backend = TestBackend::new(90, 18);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|frame| render_settings(frame, frame.area(), state, false))
        .unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn settings_header_does_not_duplicate_active_tab_label() {
    let config = AppConfig::default();
    for tab in SettingsTab::ALL {
        let mut state = SettingsState::from_config(&config);
        state.active_tab = tab;
        let dump = render_settings_to_dump(&state);
        let header = dump.lines().next().unwrap_or_default();
        assert!(
            header.contains("settings"),
            "settings header missing for {tab:?}: {header:?}"
        );
        assert!(
            !header.contains("settings ·"),
            "settings header must not duplicate active tab for {tab:?}: {header:?}"
        );
    }
}
