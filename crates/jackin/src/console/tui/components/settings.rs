//! Root-console settings display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::console::tui::components::auth_panel::settings_auth_lines_for_state;
use crate::console::tui::state::{
    GlobalMountModal, MountInfoCache, SettingsAuthModal, SettingsEnvModal, SettingsState,
    SettingsTab,
};
use jackin_console::tui::components::auth_panel::auth_panel_title;
use jackin_console::tui::components::modal_rects;
use jackin_console::tui::mount_display::format_config_mount_rows_with_cache;
use jackin_console::tui::screens::settings::view::{
    env_state_lines as settings_env_state_lines, general_lines as settings_general_lines,
    global_mount_lines as settings_global_mount_lines,
    trust_state_lines as settings_trust_state_lines,
};

pub(crate) fn render_general_tab(frame: &mut Frame<'_>, state: &SettingsState<'_>, area: Rect) {
    let focused = !state.tab_bar_focused() && state.error_popup.is_none();
    let lines = settings_general_lines(
        state.general.selected,
        state.general.pending_coauthor_trailer,
        state.general.pending_dco,
        focused,
    );
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame, area, lines, 0, 0, focused, None,
    );
}

pub(crate) fn render_mounts_tab(frame: &mut Frame<'_>, state: &SettingsState<'_>, area: Rect) {
    let focused = state.content_focused(SettingsTab::Mounts) && state.mounts.modal.is_none();
    let selected = if focused {
        Some(state.mounts.selected)
    } else {
        None
    };
    let lines = global_mount_lines_for_rows(
        &state.mounts.pending,
        selected,
        true,
        &state.mounts.mount_info_cache,
    );
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.mounts.scroll_x,
        state.mounts.scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_env_tab(frame: &mut Frame<'_>, state: &SettingsState<'_>, area: Rect) {
    let lines = settings_env_lines_for_state(state, area.width);
    let focused = state.content_focused(SettingsTab::Environments) && state.env.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.env.scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_auth_tab(frame: &mut Frame<'_>, state: &SettingsState<'_>, area: Rect) {
    let title = state
        .auth
        .selected_kind
        .map(|k| auth_panel_title(k.label()));
    let lines = settings_auth_lines_for_state(state);
    let focused = state.content_focused(SettingsTab::Auth) && state.auth.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.auth.scroll_y,
        focused,
        title.as_deref(),
    );
}

pub(crate) fn render_trust_tab(frame: &mut Frame<'_>, state: &SettingsState<'_>, area: Rect) {
    let lines = settings_trust_lines_for_state(state);
    let focused = state.content_focused(SettingsTab::Trust)
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.trust.scroll_x,
        state.trust.scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_global_mount_modal(frame: &mut Frame<'_>, modal: &GlobalMountModal<'_>) {
    let area = modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        GlobalMountModal::Text { state, .. } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        GlobalMountModal::FileBrowser { state } => {
            jackin_console::tui::components::file_browser::render(frame, area, state);
        }
        GlobalMountModal::MountDstChoice { state } => {
            jackin_console::tui::components::mount_dst_choice::render(frame, area, state);
        }
        GlobalMountModal::ScopePicker { state } => {
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        GlobalMountModal::RolePicker { state } => {
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        GlobalMountModal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
        GlobalMountModal::PreviewSave { state } => {
            jackin_console::tui::components::confirm_save::render(frame, area, state);
        }
    }
}

pub(crate) fn render_settings_env_modal(frame: &mut Frame<'_>, modal: &SettingsEnvModal<'_>) {
    let area = modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsEnvModal::Text { state, .. } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsEnvModal::SourcePicker { state } => {
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsEnvModal::OpPicker { state } => {
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        SettingsEnvModal::RolePicker { state } => {
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        SettingsEnvModal::ScopePicker { state } => {
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsEnvModal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
    }
}

pub(crate) fn render_settings_auth_modal(frame: &mut Frame<'_>, modal: &SettingsAuthModal<'_>) {
    let area = modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            crate::console::tui::components::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsAuthModal::SourcePicker { state } => {
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsAuthModal::TextInput { state } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsAuthModal::SourceFolderPicker { state } => {
            jackin_console::tui::components::file_browser::render(frame, area, state);
        }
        SettingsAuthModal::OpPicker { state } => {
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
    }
}

pub(crate) fn settings_env_lines_for_state(
    state: &SettingsState<'_>,
    area_width: u16,
) -> Vec<Line<'static>> {
    let show_cursor = state.content_focused(SettingsTab::Environments) && state.env.modal.is_none();
    settings_env_state_lines(&state.env, show_cursor, area_width)
}

pub(crate) fn settings_trust_lines_for_state(state: &SettingsState<'_>) -> Vec<Line<'static>> {
    let show_cursor = state.content_focused(SettingsTab::Trust)
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    settings_trust_state_lines(&state.trust, state.hovered_trust_row(), show_cursor)
}

pub(crate) fn global_mount_lines_for_rows(
    rows: &[crate::config::GlobalMountRow],
    selected: Option<usize>,
    include_sentinel: bool,
    cache: &MountInfoCache,
) -> Vec<Line<'static>> {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_config_mount_rows_with_cache(&mounts, cache);
    settings_global_mount_lines(&display_rows, selected, include_sentinel)
}
