#![expect(
    clippy::redundant_pub_crate,
    reason = "manager update code uses selected render geometry helpers through the moved tui facade"
)]

use crate::console::tui::components::settings::{
    render_auth_tab, render_env_tab, render_general_tab, render_mounts_tab,
    render_settings_tab_strip, render_trust_tab,
};
use crate::console::tui::state::{SettingsState, SettingsTab};
use jackin_console::tui::screens::settings::view::{settings_frame_areas, settings_header_title};
use jackin_console::tui::view::{footer_height, render_footer, render_header};
use ratatui::{Frame, layout::Rect};

pub(super) fn render_settings(
    frame: &mut Frame,
    area: Rect,
    state: &SettingsState<'_>,
    op_available: bool,
) {
    let footer = crate::console::tui::components::footer::settings::settings_footer_items(
        state,
        op_available,
    );
    let footer_h = footer_height(&footer, area.width).max(1);
    let areas = settings_frame_areas(area, footer_h);
    render_header(frame, areas.header, settings_header_title());
    render_settings_tab_strip(
        frame,
        areas.tabs,
        state.active_tab,
        state.tab_bar_focused,
        state.hovered_tab,
    );

    match state.active_tab {
        SettingsTab::General => render_general_tab(frame, state, areas.body),
        SettingsTab::Mounts => render_mounts_tab(frame, state, areas.body),
        SettingsTab::Environments => render_env_tab(frame, state, areas.body),
        SettingsTab::Auth => render_auth_tab(frame, state, areas.body),
        SettingsTab::Trust => render_trust_tab(frame, state, areas.body),
    }

    render_footer(frame, areas.footer, &footer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::console::tui::state::settings_state_from_config;
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
            let mut state = settings_state_from_config(&config);
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
}
