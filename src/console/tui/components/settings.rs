//! Root-console settings display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::console::tui::components::auth_panel::settings_auth_lines_for_state;
use crate::console::tui::components::env_value_secret_display;
use crate::console::tui::components::mount_display::format_mount_rows_with_cache;
use crate::console::tui::state::{
    GlobalMountModal, MountInfoCache, SettingsAuthModal, SettingsEnvModal, SettingsEnvScope,
    SettingsState, settings_env_flat_rows,
};
use jackin_console::tui::components::modal_rects::{self, ModalRectMode, ModalRectSpec};
use jackin_console::tui::screens::settings::view::{
    env_lines as settings_env_lines, global_mount_lines as settings_global_mount_lines,
    general_lines as settings_general_lines, trust_lines as settings_trust_lines,
};

pub(crate) fn render_general_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let focused = !state.tab_bar_focused && state.error_popup.is_none();
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

pub(crate) fn render_mounts_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let focused =
        !state.tab_bar_focused && state.mounts.scroll_focused && state.mounts.modal.is_none();
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

pub(crate) fn render_env_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let lines = settings_env_lines_for_state(state, area.width);
    let focused = !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
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

pub(crate) fn render_auth_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let title = state.auth.selected_kind.map(|k| format!(" {} ", k.label()));
    let lines = settings_auth_lines_for_state(state);
    let focused = !state.tab_bar_focused && state.auth.scroll_focused && state.auth.modal.is_none();
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

pub(crate) fn render_trust_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let lines = settings_trust_lines_for_state(state);
    let focused = !state.tab_bar_focused
        && state.trust.scroll_focused
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

pub(crate) fn render_global_mount_modal(frame: &mut Frame, modal: &GlobalMountModal<'_>) {
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

pub(crate) fn render_settings_env_modal(frame: &mut Frame, modal: &SettingsEnvModal<'_>) {
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

pub(crate) fn render_settings_auth_modal(frame: &mut Frame, modal: &SettingsAuthModal<'_>) {
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let area = modal_rects::modal_rect(
                frame.area(),
                ModalRectSpec::AuthForm {
                    required_height: crate::console::tui::components::auth_panel::required_height(
                        state,
                    ),
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
            let area = if state.naming_stage_input().is_some() {
                modal_rects::modal_rect(frame.area(), ModalRectSpec::TextInput)
            } else {
                modal_rects::modal_rect(frame.area(), ModalRectSpec::OpPicker)
            };
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
    }
}

pub(crate) fn settings_env_lines_for_state(
    state: &SettingsState<'_>,
    area_width: u16,
) -> Vec<Line<'static>> {
    let rows = settings_env_flat_rows(state);
    let show_cursor =
        !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
    settings_env_lines(
        &rows,
        state.env.selected,
        show_cursor,
        area_width,
        |scope, key| settings_env_value(state, scope, key).map(env_value_secret_display),
        |scope, key| state.env.unmasked_rows.contains(&(scope.clone(), key.to_string())),
        |role| state.env.pending.roles.get(role).map_or(0, std::collections::BTreeMap::len),
    )
}

pub(crate) fn settings_trust_lines_for_state(
    state: &SettingsState<'_>,
) -> Vec<Line<'static>> {
    let show_cursor = !state.tab_bar_focused
        && state.trust.scroll_focused
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    settings_trust_lines(
        &state.trust.pending,
        state.trust.selected,
        state.trust.hovered,
        show_cursor,
    )
}

pub(crate) fn global_mount_lines_for_rows(
    rows: &[crate::config::GlobalMountRow],
    selected: Option<usize>,
    include_sentinel: bool,
    cache: &MountInfoCache,
) -> Vec<Line<'static>> {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    settings_global_mount_lines(&display_rows, selected, include_sentinel)
}

fn settings_env_value<'a>(
    state: &'a SettingsState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a crate::operator_env::EnvValue> {
    match scope {
        SettingsEnvScope::Global => state.env.pending.env.get(key),
        SettingsEnvScope::Role(role) => state
            .env
            .pending
            .roles
            .get(role)
            .and_then(|env| env.get(key)),
    }
}
