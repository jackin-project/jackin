#![expect(
    clippy::redundant_pub_crate,
    reason = "manager update code uses selected render geometry helpers through the moved tui facade"
)]

use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
};
use crate::console::tui::components::auth_panel::settings_auth_lines_for_state;
use crate::console::tui::components::mount_display::format_mount_rows_with_cache;
use crate::console::tui::components::settings::{
    settings_env_lines_for_state, settings_trust_lines_for_state,
};
use crate::console::tui::state::{
    GlobalMountModal, MountInfoCache, SettingsAuthModal, SettingsEnvModal, SettingsState,
    SettingsTab,
};
use jackin_console::tui::components::editor_rows::render_tab_strip;
use jackin_console::tui::components::modal_rects::{self, ModalRectMode, ModalRectSpec};
use jackin_console::tui::screens::settings::view::{
    general_lines as settings_general_lines,
    global_mount_lines as settings_global_mount_lines, settings_frame_areas, tab_labels,
};
use jackin_console::tui::view::{footer_height, render_footer, render_header};

pub(super) fn render_settings(
    frame: &mut Frame,
    area: Rect,
    state: &SettingsState<'_>,
    op_available: bool,
) {
    let footer =
        crate::console::tui::components::footer::settings::settings_footer_items(state, op_available);
    let footer_h = footer_height(&footer, area.width).max(1);
    let areas = settings_frame_areas(area, footer_h);
    render_header(frame, areas.header, "settings");
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
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

fn render_general_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let focused = !state.tab_bar_focused && state.error_popup.is_none();
    let lines = settings_general_lines(
        state.general.selected,
        state.general.pending_coauthor_trailer,
        state.general.pending_dco,
        focused,
    );
    super::render_scrollable_block_at(frame, area, lines, 0, 0, focused, None);
}

fn render_mounts_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    // Only show the cursor when the mounts content block is focused — not when
    // the tab bar owns focus. Pass None as `selected` to suppress `▸` entirely.
    let focused =
        !state.tab_bar_focused && state.mounts.scroll_focused && state.mounts.modal.is_none();
    let selected = if focused {
        Some(state.mounts.selected)
    } else {
        None
    };
    let lines = global_mount_lines(
        &state.mounts.pending,
        selected,
        true,
        &state.mounts.mount_info_cache,
    );
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.mounts.scroll_x,
        state.mounts.scroll_y,
        focused,
        None,
    );
}

fn render_env_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let lines = settings_env_lines_for_state(state, area.width);
    let focused = !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
    super::render_scrollable_block_at(frame, area, lines, 0, state.env.scroll_y, focused, None);
}

fn render_auth_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let title = state.auth.selected_kind.map(|k| format!(" {} ", k.label()));
    let lines = settings_auth_lines_for_state(state);
    let focused = !state.tab_bar_focused && state.auth.scroll_focused && state.auth.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.auth.scroll_y,
        focused,
        title.as_deref(),
    );
}

fn render_trust_tab(frame: &mut Frame, state: &SettingsState<'_>, area: ratatui::layout::Rect) {
    let lines = settings_trust_lines_for_state(state);
    let focused = !state.tab_bar_focused
        && state.trust.scroll_focused
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.trust.scroll_x,
        state.trust.scroll_y,
        focused,
        None,
    );
}

fn global_mount_lines(
    rows: &[crate::config::GlobalMountRow],
    selected: Option<usize>,
    include_sentinel: bool,
    cache: &MountInfoCache,
) -> Vec<Line<'static>> {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    settings_global_mount_lines(&display_rows, selected, include_sentinel)
}

pub(super) fn render_global_mount_modal(frame: &mut Frame, modal: &GlobalMountModal<'_>) {
    match modal {
        GlobalMountModal::Text { state, .. } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::TextInput);
            jackin_tui::components::render_text_input(frame, area, state);
        }
        GlobalMountModal::FileBrowser { state } => {
            let area = modal_rects::modal_rect_for_mode(frame.area(), ModalRectMode::FileBrowser);
            jackin_console::tui::components::file_browser::render(frame, area, state);
        }
        GlobalMountModal::MountDstChoice { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::MountChoice);
            jackin_console::tui::components::mount_dst_choice::render(frame, area, state);
        }
        GlobalMountModal::ScopePicker { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::ScopePicker);
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        GlobalMountModal::RolePicker { state } => {
            let area = modal_rects::modal_rect(
                frame.area(),
                ModalRectSpec::RolePicker {
                    filtered_len: state.filtered.len(),
                },
            );
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        GlobalMountModal::Confirm { state, .. } => {
            let area = modal_rects::modal_rect(
                frame.area(),
                ModalRectSpec::Confirm {
                    width_pct: jackin_tui::components::confirm_width_pct(state),
                    height: jackin_tui::components::confirm_required_height(state),
                },
            );
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
        GlobalMountModal::PreviewSave { state } => {
            use jackin_console::tui::components::confirm_save;
            let area = modal_rects::modal_rect_for_mode(
                frame.area(),
                ModalRectMode::ConfirmSave {
                    required_height: confirm_save::required_height(state),
                },
            );
            confirm_save::render(frame, area, state);
        }
    }
}

pub(super) fn render_settings_env_modal(frame: &mut Frame, modal: &SettingsEnvModal<'_>) {
    match modal {
        SettingsEnvModal::Text { state, .. } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::TextInput);
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsEnvModal::SourcePicker { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::SourcePicker);
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsEnvModal::OpPicker { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::OpPicker);
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        SettingsEnvModal::RolePicker { state } => {
            let area = modal_rects::modal_rect(
                frame.area(),
                ModalRectSpec::RolePicker {
                    filtered_len: state.filtered.len(),
                },
            );
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        SettingsEnvModal::ScopePicker { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::ScopePicker);
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsEnvModal::Confirm { state, .. } => {
            let area = modal_rects::modal_rect(
                frame.area(),
                ModalRectSpec::Confirm {
                    width_pct: jackin_tui::components::confirm_width_pct(state),
                    height: jackin_tui::components::confirm_required_height(state),
                },
            );
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
    }
}

pub(super) fn render_settings_auth_modal(frame: &mut Frame, modal: &SettingsAuthModal<'_>) {
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let area = modal_rects::modal_rect(
                frame.area(),
                ModalRectSpec::AuthForm {
                    required_height: crate::console::tui::components::auth_panel::required_height(state),
                },
            );
            crate::console::tui::components::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsAuthModal::SourcePicker { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::SourcePicker);
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsAuthModal::TextInput { state } => {
            let area = modal_rects::modal_rect(frame.area(), ModalRectSpec::TextInput);
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsAuthModal::OpPicker { state } => {
            // A naming sub-stage is a plain input box, sized like every
            // other text-input modal; drill-down stages use the picker rect.
            let area = if state.naming_stage_input().is_some() {
                modal_rects::modal_rect(frame.area(), ModalRectSpec::TextInput)
            } else {
                modal_rects::modal_rect(frame.area(), ModalRectSpec::OpPicker)
            };
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
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
}
