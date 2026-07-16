// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings screen view helpers.

use super::model::GlobalMountConfirm;
use super::model::GlobalMountTextTarget;
use super::model::GlobalMountsState;
use super::model::SettingsAuthRow;
use super::model::SettingsAuthState;
use super::model::SettingsEnvConfig;
use super::model::SettingsEnvRow;
use super::model::SettingsEnvScope;
use super::model::SettingsEnvState;
use super::model::SettingsEnvTextTarget;
use super::model::SettingsGeneralState;
use super::model::SettingsState;
use super::model::SettingsTab;
use super::model::SettingsTrustRow;
use super::model::SettingsTrustState;
use super::update::forbidden_settings_env_keys;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::collections::BTreeMap;
use termrock::widgets::HintSpan;

use crate::tui::components::editor_rows::{
    AuthLineRow, AuthSourceDisplay, AuthSourceValue, SecretEnvLineFrame, SecretLineRow,
    SecretValueDisplay, action_row_style, auth_lines as shared_auth_lines, auth_source_display,
    render_tab_strip, secret_env_lines,
};
use crate::tui::components::footer_hints::{
    SettingsContextFooterMode, SettingsScreenFooterFacts, content_footer_items,
    settings_contextual_row_footer_items, settings_save_footer_label, settings_screen_footer_items,
    tab_bar_footer_items,
};
use crate::tui::components::mount_rows::{MOUNT_MODE_COL_WIDTH, render_global_mount_header};
use crate::tui::input::settings_auth_can_generate_token;
use crate::tui::mount_display::{
    MountDisplayRow, format_config_mount_rows_with_cache, mount_path_width,
};
use crate::tui::state::SettingsModal;
use crate::tui::view::{
    effective_footer_height, measured_footer_height, render_footer, render_header,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsFrameAreas {
    pub header: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsModalRenderPlan {
    ErrorPopup,
    Mounts,
    Environments,
    Auth,
    None,
}

pub type ConsoleSettingsState<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
> = SettingsState<
    GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
    SettingsEnvState<jackin_core::EnvValue, EnvModal>,
    SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
    SettingsTrustState,
    ErrorPopup,
    PendingToken,
>;

pub fn settings_frame_areas(area: Rect, footer_h: u16) -> SettingsFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    SettingsFrameAreas {
        header: chunks[0],
        tabs: chunks[1],
        body: chunks[2],
        footer: chunks[3],
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four orthogonal settings-modal visibility flags (error_popup, \
              mounts_modal, env_modal, auth_modal) — each is an independent \
              picker-open signal the render-plan resolver inspects to pick the \
              correct modal render target. Named-arg reads match the per-picker \
              visibility-routing idiom."
)]
#[must_use]
pub const fn settings_modal_render_plan(
    error_popup_open: bool,
    mounts_modal_open: bool,
    env_modal_open: bool,
    auth_modal_open: bool,
) -> SettingsModalRenderPlan {
    if error_popup_open {
        return SettingsModalRenderPlan::ErrorPopup;
    }
    if mounts_modal_open {
        return SettingsModalRenderPlan::Mounts;
    }
    if env_modal_open {
        return SettingsModalRenderPlan::Environments;
    }
    if auth_modal_open {
        return SettingsModalRenderPlan::Auth;
    }
    SettingsModalRenderPlan::None
}

pub fn render_settings_screen<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
    FooterItems,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    mut footer_items: FooterItems,
) where
    FooterItems: FnMut(
        &ConsoleSettingsState<
            MountModal,
            EnvModal,
            AuthModal,
            ErrorPopup,
            PendingToken,
            PendingOpCommit,
        >,
        Rect,
    ) -> Vec<HintSpan<'static>>,
{
    let provisional_body =
        settings_frame_areas(area, effective_footer_height(state.cached_footer_h)).body;
    let footer = footer_items(state, provisional_body);
    let mut footer_h = measured_footer_height(&footer, area.width);
    let mut areas = settings_frame_areas(area, footer_h);
    let mut footer = footer_items(state, areas.body);
    let exact_footer_h = measured_footer_height(&footer, area.width);
    if exact_footer_h != footer_h {
        footer_h = exact_footer_h;
        areas = settings_frame_areas(area, footer_h);
        footer = footer_items(state, areas.body);
    }
    render_header(frame, areas.header, settings_header_title());
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
        state.tab_bar_focused(),
        state.hovered_tab(),
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

pub fn render_general_tab<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    area: Rect,
) {
    let focused = !state.tab_bar_focused() && state.error_popup.is_none();
    let lines = general_state_lines(&state.general, focused);
    termrock::scroll::render_scrollable_block_at(frame, area, lines, 0, 0, focused, None);
}

pub fn render_mounts_tab<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    area: Rect,
) {
    let focused = state.content_focused(SettingsTab::Mounts) && state.mounts.modal.is_none();
    let selected = if focused {
        Some(state.mounts.selected)
    } else {
        None
    };
    let lines = global_mount_state_lines(&state.mounts, selected, true);
    termrock::scroll::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.mounts.scroll_x,
        state.mounts.scroll_y,
        focused,
        None,
    );
}

pub fn render_env_tab<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    area: Rect,
) {
    let focused = state.content_focused(SettingsTab::Environments) && state.env.modal.is_none();
    let lines = env_state_lines(&state.env, focused, area.width);
    termrock::scroll::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.env.scroll_y,
        focused,
        None,
    );
}

pub fn render_auth_tab<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    area: Rect,
) {
    let title = state
        .auth
        .selected_kind
        .map(|kind| crate::tui::components::auth_panel::auth_panel_title(kind.label()));
    let focused = state.content_focused(SettingsTab::Auth) && state.auth.modal.is_none();
    let lines = auth_state_lines(&state.auth, &state.env, focused);
    termrock::scroll::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.auth.scroll_y,
        focused,
        title.as_deref(),
    );
}

pub fn render_trust_tab<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    area: Rect,
) {
    let lines = settings_trust_lines_for_state(state);
    let focused = settings_trust_focused(state);
    termrock::scroll::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.trust.scroll_x,
        state.trust.scroll_y,
        focused,
        None,
    );
}

pub fn settings_footer_items<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if state.tab_bar_focused() {
        return tab_bar_footer_items(
            settings_save_footer_label(),
            true,
            state.is_dirty().then(|| state.change_count()),
        );
    }

    let row_items = settings_contextual_row_footer_items(
        settings_context_footer_mode(state, body_area),
        op_available,
    );
    content_footer_items(
        settings_save_footer_label(),
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}

fn settings_context_footer_mode<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    body_area: Rect,
) -> SettingsContextFooterMode {
    match state.active_tab {
        SettingsTab::General => SettingsContextFooterMode::General,
        SettingsTab::Mounts => {
            let cursor = state.mounts.selected;
            let mount_count = state.mounts.pending.len();
            if cursor == mount_count {
                SettingsContextFooterMode::MountAddRow
            } else {
                SettingsContextFooterMode::MountRow {
                    has_github_url: state
                        .mounts
                        .pending
                        .get(cursor)
                        .and_then(|row| {
                            state.mounts.mount_info_cache.github_web_url(&row.mount.src)
                        })
                        .is_some(),
                    scroll_axes: global_mount_scroll_axes(state, body_area),
                }
            }
        }
        SettingsTab::Environments => {
            let rows = state.env_flat_rows();
            match rows.get(state.env.selected) {
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_env_value_is_op_ref(state, scope, key) =>
                {
                    SettingsContextFooterMode::EnvOpRefRow
                }
                Some(SettingsEnvRow::Key { .. }) => SettingsContextFooterMode::EnvPlainRow,
                Some(SettingsEnvRow::RoleHeader { .. }) => SettingsContextFooterMode::EnvRoleHeader,
                Some(SettingsEnvRow::GlobalAddSentinel | SettingsEnvRow::RoleAddSentinel(_)) => {
                    SettingsContextFooterMode::EnvAddRow
                }
                Some(SettingsEnvRow::SectionSpacer) | None => SettingsContextFooterMode::Empty,
            }
        }
        SettingsTab::Auth => {
            if state.auth.selected_kind.is_none() {
                SettingsContextFooterMode::AuthManage
            } else if state.auth.selected_detail_row_is_focusable() {
                SettingsContextFooterMode::AuthEditMode
            } else {
                SettingsContextFooterMode::Empty
            }
        }
        SettingsTab::Trust => SettingsContextFooterMode::Trust {
            has_roles: !state.trust.pending.is_empty(),
            scroll_axes: trust_scroll_axes(state, body_area),
        },
    }
}

fn trust_scroll_axes<MountModal, EnvModal, AuthModal, ErrorPopup, PendingToken, PendingOpCommit>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    body_area: Rect,
) -> termrock::layout::ScrollAxes {
    let content = crate::tui::screens::settings::update::trust_content_width(&state.trust);
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.trust.pending.is_empty(),
        content,
        body_area,
    )
}

fn global_mount_scroll_axes<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    body_area: Rect,
) -> termrock::layout::ScrollAxes {
    let content_width =
        crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache(
            &state.mounts.pending,
            &state.mounts.mount_info_cache,
        );
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.mounts.pending.is_empty(),
        content_width,
        body_area,
    )
}

fn settings_env_value_is_op_ref<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    scope: &SettingsEnvScope,
    key: &str,
) -> bool {
    state
        .env
        .pending_value(scope, key)
        .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
}

pub fn render_global_mount_modal(frame: &mut Frame<'_>, modal: &SettingsModal<'_>) {
    let area =
        crate::tui::components::modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsModal::MountText { state, .. } => {
            crate::tui::components::render_text_input(frame, area, state);
        }
        SettingsModal::MountFileBrowser { state } => {
            crate::tui::components::file_browser::render(frame, area, state);
        }
        SettingsModal::MountDstChoice { state } => {
            crate::tui::components::mount_dst_choice::render(frame, area, state);
        }
        SettingsModal::MountScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsModal::MountRolePicker { state } => {
            crate::tui::components::role_picker::render(frame, area, state);
        }
        SettingsModal::MountConfirm { state, .. } => {
            crate::tui::components::render_confirm_dialog(frame, area, state);
        }
        SettingsModal::MountPreviewSave { state } => {
            crate::tui::components::confirm_save::render(frame, area, state);
        }
        _ => unreachable!("mount renderer received a non-mount settings modal"),
    }
}

pub fn render_settings_env_modal(frame: &mut Frame<'_>, modal: &SettingsModal<'_>) {
    let area =
        crate::tui::components::modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsModal::EnvText { state, .. } => {
            crate::tui::components::render_text_input(frame, area, state);
        }
        SettingsModal::EnvSourcePicker { state, .. } => {
            crate::tui::components::source_picker::render(frame, area, state);
        }
        SettingsModal::EnvOpPicker { state, .. } => {
            crate::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        SettingsModal::EnvRolePicker { state } => {
            crate::tui::components::role_picker::render(frame, area, state);
        }
        SettingsModal::EnvScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsModal::EnvConfirm { state, .. } => {
            crate::tui::components::render_confirm_dialog(frame, area, state);
        }
        _ => unreachable!("env renderer received a non-env settings modal"),
    }
}

pub fn render_settings_auth_modal(frame: &mut Frame<'_>, modal: &SettingsModal<'_>) {
    let area =
        crate::tui::components::modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsModal::AuthForm { state, focus, .. } => {
            crate::tui::components::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsModal::AuthSourcePicker { state } => {
            crate::tui::components::source_picker::render(frame, area, state);
        }
        SettingsModal::AuthTextInput { state } => {
            crate::tui::components::render_text_input(frame, area, state);
        }
        SettingsModal::AuthSourceFolderPicker { state } => {
            crate::tui::components::file_browser::render(frame, area, state);
        }
        SettingsModal::AuthOpPicker { state } => {
            crate::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        _ => unreachable!("auth renderer received a non-auth settings modal"),
    }
}

pub fn settings_env_lines_for_state<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
    area_width: u16,
) -> Vec<Line<'static>> {
    let show_cursor = state.content_focused(SettingsTab::Environments) && state.env.modal.is_none();
    env_state_lines(&state.env, show_cursor, area_width)
}

pub fn settings_trust_lines_for_state<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
) -> Vec<Line<'static>> {
    trust_state_lines(
        &state.trust,
        state.hovered_trust_row(),
        settings_trust_focused(state),
    )
}

fn settings_trust_focused<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
) -> bool {
    state.content_focused(SettingsTab::Trust)
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none()
}

#[must_use]
pub fn content_height_with_error_rows(height: usize, has_error: bool) -> usize {
    if has_error {
        height.saturating_add(2)
    } else {
        height
    }
}

#[must_use]
pub fn mounts_content_height(row_height: usize, has_error: bool) -> usize {
    content_height_with_error_rows(row_height, has_error)
}

#[must_use]
pub fn env_content_height(row_count: usize, has_error: bool) -> usize {
    content_height_with_error_rows(row_count, has_error)
}

#[must_use]
pub fn trust_content_height(row_count: usize, has_error: bool) -> usize {
    content_height_with_error_rows(1 + row_count.max(1), has_error)
}

#[must_use]
pub fn general_lines(
    selected_row: usize,
    pending_coauthor_trailer: bool,
    pending_dco: bool,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    use crate::tui::screens::form_model::{FieldRow, FormSection};
    FormSection::new(
        vec![
            FieldRow::new(
                "Co-author trailer",
                if pending_coauthor_trailer {
                    "enabled"
                } else {
                    "disabled"
                },
            ),
            FieldRow::new(
                "DCO sign-off",
                if pending_dco { "enabled" } else { "disabled" },
            ),
        ],
        selected_row,
        show_cursor,
        26,
    )
    .lines()
}

#[must_use]
pub fn general_state_lines(state: &SettingsGeneralState, show_cursor: bool) -> Vec<Line<'static>> {
    general_lines(
        state.selected,
        state.pending_coauthor_trailer,
        state.pending_dco,
        show_cursor,
    )
}

#[must_use]
pub fn trust_lines(
    rows: &[SettingsTrustRow],
    selected_row: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "  Role                         Trust      Git",
        Style::default().fg(jackin_core::tui_theme::WHITE),
    ))];
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(jackin_core::tui_theme::PHOSPHOR_DIM),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (selected_row == i);
        let mut style = if selected {
            Style::default()
                .fg(termrock::style::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(termrock::style::PHOSPHOR_GREEN)
        };
        if !selected && hovered_row == Some(i) {
            style = style.bg(jackin_core::tui_theme::TAB_BG_INACTIVE_HOVER);
        }
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let trust = if row.trusted { "trusted" } else { "untrusted" };
        lines.push(Line::from(Span::styled(
            format!(
                "{prefix}{:<28} {:<10} {}",
                truncate(&row.role, 28),
                trust,
                row.git
            ),
            style,
        )));
    }
    lines
}

#[must_use]
pub fn trust_state_lines(
    state: &SettingsTrustState,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    trust_lines(&state.pending, state.selected, hovered_row, show_cursor)
}

#[must_use]
pub fn env_lines<'a>(
    rows: &[SettingsEnvRow],
    selected_row: usize,
    show_cursor: bool,
    area_width: u16,
    value_for: impl Fn(&SettingsEnvScope, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SettingsEnvScope, &str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> Vec<Line<'static>> {
    let display_rows: Vec<SecretLineRow<SettingsEnvScope>> = rows
        .iter()
        .map(|row| match row {
            SettingsEnvRow::Key { scope, key } => SecretLineRow::Key {
                scope: scope.clone(),
                key: key.clone(),
            },
            SettingsEnvRow::GlobalAddSentinel => SecretLineRow::WorkspaceAddSentinel,
            SettingsEnvRow::RoleHeader { role, expanded } => SecretLineRow::RoleHeader {
                role: role.clone(),
                expanded: *expanded,
            },
            SettingsEnvRow::RoleAddSentinel(role) => SecretLineRow::RoleAddSentinel(role.clone()),
            SettingsEnvRow::SectionSpacer => SecretLineRow::SectionSpacer,
        })
        .collect();
    secret_env_lines(
        &display_rows,
        SecretEnvLineFrame {
            cursor: selected_row,
            show_cursor,
            area_width,
        },
        value_for,
        is_unmasked,
        |_| true,
        role_var_count,
    )
}

#[must_use]
pub fn env_state_lines<Modal>(
    state: &SettingsEnvState<jackin_core::EnvValue, Modal>,
    show_cursor: bool,
    area_width: u16,
) -> Vec<Line<'static>> {
    let rows = crate::tui::screens::settings::update::settings_env_flat_rows(
        &state.pending,
        &state.expanded,
    );
    env_lines(
        &rows,
        state.selected,
        show_cursor,
        area_width,
        |scope, key| {
            state
                .pending_value(scope, key)
                .map(crate::tui::components::env_value::secret_display)
        },
        |scope, key| state.is_unmasked(scope, key),
        |role| state.pending.roles.get(role).map_or(0, BTreeMap::len),
    )
}

#[must_use]
pub fn auth_lines(
    rows: &[AuthLineRow],
    selected_row: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    shared_auth_lines(rows, selected_row, show_cursor)
}

#[must_use]
pub fn auth_state_lines<AuthModal, EnvModal, PendingOpCommit>(
    auth: &SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
    env: &SettingsEnvState<jackin_core::EnvValue, EnvModal>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let Some(kind) = auth.selected_kind else {
        let rows: Vec<AuthLineRow> = auth
            .pending
            .iter()
            .map(|row| AuthLineRow::AuthKind {
                label: row.kind.label().to_owned(),
            })
            .collect();
        return auth_lines(&rows, auth.selected, show_cursor);
    };

    let Some(row) = auth.pending.iter().find(|row| row.kind == kind) else {
        return Vec::new();
    };

    let mut rows = vec![AuthLineRow::WorkspaceMode {
        mode_label: crate::tui::components::auth_panel::mode_str(row.mode).to_owned(),
        inherited: false,
    }];
    if let Some(env_name) = kind.required_env_var(row.mode) {
        rows.push(AuthLineRow::WorkspaceSource {
            display: settings_auth_source_display(auth, env, kind, row.mode, env_name),
        });
    }
    if crate::tui::auth::auth_mode_supports_source_folder(kind, row.mode) {
        rows.push(AuthLineRow::WorkspaceSourceFolder {
            display: crate::tui::auth_config::settings_source_folder_display(row),
        });
    }
    rows.push(AuthLineRow::Spacer);
    auth_lines(&rows, auth.selected, show_cursor)
}

fn settings_auth_source_display<AuthModal, EnvModal, PendingOpCommit>(
    auth: &SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
    env: &SettingsEnvState<jackin_core::EnvValue, EnvModal>,
    kind: crate::tui::auth::AuthKind,
    mode: crate::tui::auth::AuthMode,
    env_name: &str,
) -> AuthSourceDisplay {
    auth_source_display(
        settings_auth_source_value(auth, env, kind, mode).map(|value| match value {
            jackin_core::EnvValue::Plain(value) => AuthSourceValue::Plain(value.clone()),
            jackin_core::EnvValue::Extended(e) => AuthSourceValue::Plain(e.value.clone()),
            jackin_core::EnvValue::OpRef(op_ref) => AuthSourceValue::OpRefPath(op_ref.path.clone()),
        }),
        env_name,
        crate::tui::components::auth_panel::mode_str(mode),
    )
}

fn settings_auth_source_value<'a, AuthModal, EnvModal, PendingOpCommit>(
    auth: &'a SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
    env: &'a SettingsEnvState<jackin_core::EnvValue, EnvModal>,
    kind: crate::tui::auth::AuthKind,
    mode: crate::tui::auth::AuthMode,
) -> Option<&'a jackin_core::EnvValue> {
    crate::tui::auth_config::settings_auth_env_value(kind, mode, &auth.github_env, &env.pending.env)
}

#[must_use]
pub fn global_mount_lines(
    rows: &[MountDisplayRow],
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if !rows.is_empty() {
        lines.push(render_global_mount_header(path_w));
    }
    for (i, row) in rows.iter().enumerate() {
        let is_selected = selected == Some(i);
        let prefix = if is_selected { "\u{25b8} " } else { "  " };
        let base_style = if is_selected {
            Style::default()
                .fg(termrock::style::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(termrock::style::PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(jackin_core::tui_theme::PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{:<path_w$}  ", row.destination),
                base_style,
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(jackin_core::tui_theme::PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(row.kind.clone(), dim_style),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(jackin_core::tui_theme::PHOSPHOR_DIM),
            )));
        }
    }
    if include_sentinel {
        let sentinel_selected = selected == Some(rows.len());
        let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
        if !rows.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("{sentinel_prefix}+ Add mount"),
            action_row_style(sentinel_selected),
        )));
    }
    lines
}

#[must_use]
pub fn global_mount_state_lines<Modal>(
    state: &GlobalMountsState<jackin_config::GlobalMountRow, Modal>,
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let mounts = state
        .pending
        .iter()
        .map(|row| row.mount.clone())
        .collect::<Vec<_>>();
    let display_rows = format_config_mount_rows_with_cache(&mounts, &state.mount_info_cache);
    global_mount_lines(&display_rows, selected, include_sentinel)
}

fn truncate(value: &str, width: usize) -> String {
    let mut out: String = value.chars().take(width).collect();
    if value.chars().count() > width && width > 1 {
        out.pop();
        out.push('\u{2026}');
    }
    out
}

pub fn clamp_mounts_scroll_x_for_frame(area: Rect, content_width: usize, scroll_x: &mut u16) {
    let areas = settings_frame_areas(area, 2);
    termrock::scroll::clamp_scroll_offset(
        content_width,
        termrock::scroll::viewport_width(areas.body),
        scroll_x,
    );
}

#[must_use]
pub fn auth_content_height<K, M>(
    selected_kind: Option<K>,
    rows: &[SettingsAuthRow<K, M>],
    detail_row_count: impl Fn(K, &M) -> usize,
    has_error: bool,
) -> usize
where
    K: Copy + PartialEq,
{
    let height = match selected_kind {
        None => rows.len(),
        Some(kind) => rows
            .iter()
            .find(|row| row.kind == kind)
            .map_or(0, |row| 1 + detail_row_count(kind, &row.mode)),
    };
    content_height_with_error_rows(height, has_error)
}

/// Concrete adapter: render the settings screen for a concrete `SettingsState`.
pub fn render_settings_with_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::state::SettingsState<'_>,
    op_available: bool,
) {
    render_settings_screen(frame, area, state, |state, body| {
        settings_screen_footer_for_state(state, op_available, body)
    });
}

/// Concrete adapter: compose settings footer items for a concrete `SettingsState`.
///
/// Gives modals priority over screen items, so whatever is active on-screen
/// gets the footer real-estate. The generic `settings_footer_items` handles
/// per-screen hint routing; this function layers modal items on top.
#[must_use]
pub fn settings_screen_footer_for_state(
    state: &crate::tui::state::SettingsState<'_>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    settings_screen_footer_items(SettingsScreenFooterFacts {
        auth_modal_items: state
            .auth
            .modal_ref()
            .map(|modal| modal.auth_footer_items(settings_auth_can_generate_token(&state.auth))),
        env_modal_items: state
            .env
            .modal
            .as_ref()
            .map(SettingsModal::env_footer_items),
        mounts_modal_items: state
            .mounts
            .modal
            .as_ref()
            .map(SettingsModal::mounts_footer_items),
        screen_items: settings_footer_items(state, op_available, body_area),
    })
}

mod text_helpers;
pub use text_helpers::*;

#[cfg(test)]
mod tests;
