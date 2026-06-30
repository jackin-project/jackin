//! Editor screen view helpers.

use super::model::{
    AuthRow, EditorMode, EditorState, EditorTab, FieldFocus, SecretsRow, SecretsScopeTag,
};
use super::update::forbidden_secret_keys;
use crate::tui::components::editor_rows::{
    AUTH_LABEL_COL_WIDTH, AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind,
    AuthSourceValue, action_row_style, auth_source_display_for_required_env,
    disclosure_style, render_tab_strip,
};

use crate::tui::components::footer_hints::{
    EditorContextFooterMode, editor_contextual_row_footer_items,
};


use crate::tui::view::{
    effective_footer_height, measured_footer_height, render_footer, render_header,
};
use jackin_tui::HintSpan;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

// Structural exception: editor rows are form/table rows with labels, values,
// disclosures, masked secrets, and action sentinels, so they cannot use the
// flat picker renderer even though they share its focus-gated cursor contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthLineRow {
    AuthKind { label: String },
    WorkspaceMode { mode_label: String, inherited: bool },
    WorkspaceSource { display: AuthSourceDisplay },
    WorkspaceSourceFolder { display: AuthSourceFolderDisplay },
    RoleHeader { role: String, expanded: bool },
    RoleMode { mode_label: String },
    RoleSource { display: AuthSourceDisplay },
    RoleSourceFolder { display: AuthSourceFolderDisplay },
    AddSentinel { eligible: usize },
    Spacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorScrollGeometry {
    pub active_mounts: bool,
    pub content_width: usize,
    pub content_height: usize,
    pub mounts_content_width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTabContentGeometry {
    pub content_width: usize,
    pub content_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorFrameAreas {
    pub header: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

pub type WorkspaceEditorState<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> = EditorState<
    jackin_config::WorkspaceConfig,
    crate::mount_info_cache::MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>;

mod general_tab;
#[allow(unused_imports)]
pub(crate) use general_tab::{
    editor_general_content_width, editor_row_width, general_lines, general_row_widths,
};

mod mounts_tab;

mod roles_tab;
#[allow(unused_imports)]
pub(crate) use roles_tab::{
    EditorRoleRow, editor_role_load_row_width, editor_role_row_width,
    editor_roles_status_width, role_lines, role_state_geometry, role_state_lines,
};

mod secrets_tab;
#[allow(unused_imports)]
pub(crate) use secrets_tab::{
    editor_secret_line_width, secret_key_line_width, secret_lines, secret_state_geometry,
    secret_state_lines,
};

pub fn editor_frame_areas(area: Rect, footer_h: u16) -> EditorFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    EditorFrameAreas {
        header: chunks[0],
        tabs: chunks[1],
        body: chunks[2],
        footer: chunks[3],
    }
}

#[allow(clippy::type_complexity)]
pub fn render_editor_screen<
    Modal,
    SaveFlow,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
    FooterItems,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    mut footer_items: FooterItems,
) where
    FooterItems: FnMut(
        &WorkspaceEditorState<
            Modal,
            SaveFlow,
            jackin_core::EnvValue,
            AuthFormTarget,
            PendingTokenGenerate,
            PendingRoleLoad,
            PendingDriftCheck,
            PendingIsolationCleanup,
            PendingOpCommit,
        >,
        &jackin_config::AppConfig,
        Rect,
    ) -> Vec<HintSpan<'static>>,
{
    let provisional_body =
        editor_frame_areas(area, effective_footer_height(state.cached_footer_h)).body;
    let items = footer_items(state, config, provisional_body);
    let mut footer_h = measured_footer_height(&items, area.width);
    let mut areas = editor_frame_areas(area, footer_h);
    let mut items = footer_items(state, config, areas.body);
    let exact_footer_h = measured_footer_height(&items, area.width);
    if exact_footer_h != footer_h {
        footer_h = exact_footer_h;
        areas = editor_frame_areas(area, footer_h);
        items = footer_items(state, config, areas.body);
    }

    let title = editor_header_title(&state.mode);
    render_header(frame, areas.header, &title);
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
        state.tab_bar_focused(),
        state.hovered_tab(),
    );

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, areas.body, state),
        EditorTab::Mounts => render_mounts_tab(frame, areas.body, state),
        EditorTab::Roles => render_roles_tab(frame, areas.body, state, config),
        EditorTab::Secrets => render_secrets_tab(frame, areas.body, state, config),
        EditorTab::Auth => render_auth_tab(frame, areas.body, state, config),
    }

    render_footer(frame, areas.footer, &items);
}

#[allow(clippy::type_complexity)]
pub fn editor_contextual_footer_items<
    Modal,
    SaveFlow,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    editor_contextual_row_footer_items(
        editor_context_footer_mode(state, config, body_area),
        op_available,
    )
}

#[allow(clippy::type_complexity)]
fn editor_context_footer_mode<
    Modal,
    SaveFlow,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    body_area: Rect,
) -> EditorContextFooterMode {
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => EditorContextFooterMode::General {
            row: cursor,
            has_mounts: !state.pending.mounts.is_empty(),
        },
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            match cursor.cmp(&mount_count) {
                std::cmp::Ordering::Less => EditorContextFooterMode::MountRow {
                    has_github_url: state
                        .pending
                        .mounts
                        .get(cursor)
                        .and_then(|m| state.mount_info_cache.github_web_url(&m.src))
                        .is_some(),
                    scroll_axes: workspace_mount_scroll_axes(state, body_area),
                },
                std::cmp::Ordering::Equal => EditorContextFooterMode::MountAddRow,
                std::cmp::Ordering::Greater => EditorContextFooterMode::Empty,
            }
        }
        EditorTab::Roles => EditorContextFooterMode::RoleRow {
            is_existing_role: cursor < config.roles.len(),
        },
        EditorTab::Secrets => {
            let rows = state.secrets_flat_rows();
            let focused_value_is_op_ref = match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(key)) => state
                    .pending
                    .env
                    .get(key)
                    .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
                Some(SecretsRow::RoleKeyRow { role, key }) => state
                    .pending
                    .roles
                    .get(role)
                    .and_then(|ov| ov.env.get(key))
                    .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
                _ => false,
            };
            match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. })
                    if focused_value_is_op_ref =>
                {
                    EditorContextFooterMode::SecretOpRefRow
                }
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. }) => {
                    EditorContextFooterMode::SecretPlainRow
                }
                Some(SecretsRow::RoleHeader { .. }) => EditorContextFooterMode::SecretRoleHeader,
                Some(SecretsRow::WorkspaceAddSentinel | SecretsRow::RoleAddSentinel(_)) => {
                    EditorContextFooterMode::SecretAddRow
                }
                Some(SecretsRow::SectionSpacer) | None => EditorContextFooterMode::Empty,
            }
        }
        EditorTab::Auth => {
            let flat = state.auth_flat_rows(config);
            match flat.get(cursor) {
                Some(AuthRow::AuthKindRow { .. }) => EditorContextFooterMode::AuthManage,
                Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                    EditorContextFooterMode::AuthEditMode
                }
                Some(AuthRow::RoleHeader { .. }) => EditorContextFooterMode::AuthRoleHeader,
                Some(AuthRow::AddSentinel { .. }) => EditorContextFooterMode::AuthAddOverride,
                Some(
                    AuthRow::WorkspaceSource { .. }
                    | AuthRow::RoleSource { .. }
                    | AuthRow::WorkspaceSourceFolder { .. }
                    | AuthRow::RoleSourceFolder { .. }
                    | AuthRow::Spacer,
                )
                | None => EditorContextFooterMode::Empty,
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn workspace_mount_scroll_axes<
    Modal,
    SaveFlow,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    body_area: Rect,
) -> jackin_tui::components::ScrollAxes {
    let content_width = crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
        &state.pending.mounts,
        &state.mount_info_cache,
    );
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.pending.mounts.is_empty(),
        content_width,
        body_area,
    )
}

#[allow(clippy::type_complexity)]
pub fn render_general_tab<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) {
    let rows = editor_general_lines_for_state(state);
    let focused = editor_tab_content_focused(state);
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        rows,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

#[allow(clippy::type_complexity)]
pub fn render_mounts_tab<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) {
    let lines = editor_mount_lines_for_state(state);
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.workspace_mounts_scroll_x,
        state.tab_scroll_y,
        state.workspace_mounts_scroll_focused() && state.modal.is_none(),
        None,
    );
}

#[allow(clippy::type_complexity)]
pub fn render_roles_tab<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let lines = editor_role_lines_for_state(state, config);
    let focused = editor_tab_content_focused(state);
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

#[allow(clippy::type_complexity)]
pub fn render_secrets_tab<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let lines = editor_secret_lines_for_state(area, state, config);
    let focused = editor_tab_content_focused(state);
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

#[allow(clippy::type_complexity)]
pub fn render_auth_tab<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let lines = editor_auth_lines_for_state(state, config);
    let title = state
        .auth_selected_kind
        .map(|kind| crate::tui::components::auth_panel::auth_panel_title(kind.label()));
    let focused = editor_tab_content_focused(state);
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        title.as_deref(),
    );
}

#[allow(clippy::type_complexity)]
fn editor_tab_content_focused<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> bool {
    !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none()
}

#[allow(clippy::type_complexity)]
pub fn editor_general_lines_for_state<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> Vec<Line<'static>> {
    general_tab::general_state_lines(state, editor_tab_content_focused(state))
}

#[allow(clippy::type_complexity)]
pub fn editor_mount_lines_for_state<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> Vec<Line<'static>> {
    let show_cursor = !state.tab_bar_focused()
        && state.workspace_mounts_scroll_focused()
        && state.modal.is_none();
    mounts_tab::mount_state_lines(state, show_cursor)
}

#[allow(clippy::type_complexity)]
pub fn editor_role_lines_for_state<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> Vec<Line<'static>> {
    roles_tab::role_state_lines(
        state,
        config.roles.keys(),
        editor_tab_content_focused(state),
    )
}

#[allow(clippy::type_complexity)]
pub fn editor_secret_lines_for_state<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> Vec<Line<'static>> {
    secrets_tab::secret_state_lines(
        state,
        editor_tab_content_focused(state),
        area.width,
        |role| config.roles.contains_key(role),
    )
}

#[allow(clippy::type_complexity)]
pub fn editor_auth_lines_for_state<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> Vec<Line<'static>> {
    auth_state_lines(state, config, editor_tab_content_focused(state))
}

#[allow(clippy::type_complexity)]
pub fn prepare_editor_for_render<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    area: Rect,
    state: &mut WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let body = editor_body_area(area, state.cached_footer_h);
    prepare_editor_tab_for_area(body, state, config);
}

#[allow(clippy::type_complexity)]
pub fn prepare_editor_tab_for_area<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    body: Rect,
    state: &mut WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let geometry = editor_tab_geometry(body, state, config);
    state.tab_content_width = geometry.content_width;
    state.tab_content_height = geometry.content_height;
    clamp_editor_scroll_for_frame(
        body,
        EditorScrollGeometry {
            active_mounts: state.active_tab == EditorTab::Mounts,
            content_width: geometry.content_width,
            content_height: geometry.content_height,
            mounts_content_width:
                crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
                    &state.pending.mounts,
                    &state.mount_info_cache,
                ),
        },
        &mut state.tab_scroll_x,
        &mut state.tab_scroll_y,
        &mut state.workspace_mounts_scroll_x,
    );
}

#[must_use]
#[allow(clippy::type_complexity)]
pub fn editor_tab_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> EditorTabContentGeometry {
    match state.active_tab {
        EditorTab::General => general_tab::general_state_geometry(state),
        EditorTab::Mounts => mounts_tab::mount_state_geometry(state),
        EditorTab::Roles => roles_tab::role_state_geometry(state, config.roles.keys()),
        EditorTab::Secrets => {
            secrets_tab::secret_state_geometry(state, area.width, |role| config.roles.contains_key(role))
        }
        EditorTab::Auth => auth_state_geometry(state, config),
    }
}

#[must_use]
pub fn editor_header_title(mode: &EditorMode) -> String {
    match mode {
        EditorMode::Edit { name } => format!("edit workspace · {name}"),
        EditorMode::Create => "create workspace".to_owned(),
    }
}

#[must_use]
pub fn editor_name_value(
    mode: &EditorMode,
    pending_name: Option<&str>,
    create_fallback: &str,
) -> String {
    match mode {
        EditorMode::Edit { name } => pending_name.unwrap_or(name).to_owned(),
        EditorMode::Create => pending_name.unwrap_or(create_fallback).to_owned(),
    }
}

#[must_use]
pub fn secret_delete_confirm_prompt(key: &str) -> String {
    format!("Delete environment variable {key}?")
}

#[must_use]
pub fn secret_delete_confirm_state(key: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(secret_delete_confirm_prompt(key))
}

#[must_use]
pub fn editor_name_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Rename workspace", current)
}

#[must_use]
pub fn editor_workdir_pick_state<M: crate::tui::components::workdir_pick::WorkdirMount>(
    mounts: &[M],
) -> crate::tui::components::workdir_pick::WorkdirPickState {
    crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(mounts)
}

#[must_use]
pub fn secret_value_input_state<'a>(
    key: &str,
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new_allow_empty(format!("Edit {key}"), current)
}

#[must_use]
pub fn secret_value_current_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

#[must_use]
pub fn secret_new_value_input_state<'a>(key: &str) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new_allow_empty(
        format!("Value for {key}"),
        String::new(),
    )
}

#[must_use]
pub fn secret_source_picker_state(
    key: impl Into<String>,
    op_available: bool,
) -> crate::tui::components::source_picker::SourcePickerState {
    crate::tui::components::source_picker::SourcePickerState::new(key.into(), op_available)
}

#[must_use]
pub fn secret_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::new()
}

#[must_use]
pub fn secret_new_key_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "New workspace environment key".to_owned(),
        SecretsScopeTag::Role(role) => format!("New {role} environment key"),
    }
}

#[must_use]
pub fn secret_new_key_after_picker_label(scope: &SecretsScopeTag) -> String {
    format!("New environment key for {}", secrets_scope_label(scope))
}

#[must_use]
pub fn secret_empty_key_label() -> &'static str {
    "Key cannot be empty"
}

#[must_use]
pub fn role_load_input_state<'a>(
    trusted_roles: Vec<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state =
        jackin_tui::components::TextInputState::new_with_forbidden("Load role", "", trusted_roles);
    state.forbidden_label = "trusted role registry".into();
    state
}

#[must_use]
pub fn mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn mount_dst_choice_state(
    src: impl Into<String>,
) -> crate::tui::components::mount_dst_choice::MountDstChoiceState {
    crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src)
}

#[must_use]
pub fn role_trust_confirm_state(
    role: String,
    repository: String,
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::details(
        "Trust role source",
        "Trust this role source?",
        vec![("Role".into(), role), ("Repository".into(), repository)],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    )
}

#[must_use]
pub fn isolated_state_save_confirm_state(
    affected_containers: &[String],
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!(
        "Edit affects preserved isolated state for {} stopped container(s):\n  {}\n\n\
         Delete the preserved state and save?",
        affected_containers.len(),
        affected_containers.join("\n  "),
    ))
}

#[must_use]
pub fn running_isolated_state_save_block_message(affected_containers: &[String]) -> String {
    format!(
        "Cannot save: {} container(s) are running with isolated state for an affected mount: {}; eject them first.",
        affected_containers.len(),
        affected_containers.join(", "),
    )
}

pub fn clamp_editor_scroll_for_frame(
    body: Rect,
    geometry: EditorScrollGeometry,
    tab_scroll_x: &mut u16,
    tab_scroll_y: &mut u16,
    mounts_scroll_x: &mut u16,
) {
    let viewport_w = jackin_tui::components::scrollable_panel::viewport_width(body);
    let viewport_h = jackin_tui::components::scrollable_panel::viewport_height(body);
    if geometry.active_mounts {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            geometry.mounts_content_width,
            viewport_w,
            mounts_scroll_x,
        );
    } else {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            geometry.content_width,
            viewport_w,
            tab_scroll_x,
        );
    }
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        geometry.content_height,
        viewport_h,
        tab_scroll_y,
    );
}

pub fn editor_body_area(area: Rect, footer_h: u16) -> Rect {
    editor_frame_areas(area, footer_h).body
}

#[must_use]
pub fn auth_lines(
    rows: &[EditorAuthLineRow],
    cursor: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| render_auth_line(show_cursor && (i == cursor), row))
        .collect()
}

#[must_use]
pub fn auth_display_row(
    row: &AuthRow<crate::tui::auth::AuthKind>,
    synthesized: &jackin_config::AppConfig,
    workspace_name: &str,
) -> EditorAuthLineRow {
    match row {
        AuthRow::AuthKindRow { kind } => EditorAuthLineRow::AuthKind {
            label: kind.label().to_owned(),
        },
        AuthRow::WorkspaceMode { kind } => {
            let ws = synthesized.workspaces.get(workspace_name);
            let explicit =
                ws.and_then(|ws| crate::tui::auth_config::explicit_workspace_auth_mode(ws, *kind));
            let mode = explicit.unwrap_or_else(|| {
                crate::tui::auth_config::resolve_panel_mode(synthesized, *kind, workspace_name, "")
            });
            EditorAuthLineRow::WorkspaceMode {
                mode_label: crate::tui::components::auth_panel::mode_str(mode).to_owned(),
                inherited: explicit.is_none(),
            }
        }
        AuthRow::WorkspaceSource { kind } => EditorAuthLineRow::WorkspaceSource {
            display: editor_auth_source_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::WorkspaceSourceFolder { kind } => EditorAuthLineRow::WorkspaceSourceFolder {
            display: crate::tui::auth_config::editor_source_folder_display(
                synthesized,
                workspace_name,
                "",
                *kind,
            ),
        },
        AuthRow::RoleHeader { role, expanded } => EditorAuthLineRow::RoleHeader {
            role: role.clone(),
            expanded: *expanded,
        },
        AuthRow::RoleMode { role, kind } => {
            let mode = crate::tui::auth_config::resolve_panel_mode(
                synthesized,
                *kind,
                workspace_name,
                role,
            );
            EditorAuthLineRow::RoleMode {
                mode_label: crate::tui::components::auth_panel::mode_str(mode).to_owned(),
            }
        }
        AuthRow::RoleSource { role, kind } => EditorAuthLineRow::RoleSource {
            display: editor_auth_source_display(synthesized, workspace_name, role, *kind),
        },
        AuthRow::RoleSourceFolder { role, kind } => EditorAuthLineRow::RoleSourceFolder {
            display: crate::tui::auth_config::editor_source_folder_display(
                synthesized,
                workspace_name,
                role,
                *kind,
            ),
        },
        AuthRow::AddSentinel { eligible } => EditorAuthLineRow::AddSentinel {
            eligible: *eligible,
        },
        AuthRow::Spacer => EditorAuthLineRow::Spacer,
    }
}

#[must_use]
#[allow(clippy::type_complexity)]
pub fn auth_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let rows = state.auth_flat_rows(config);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = rows.len().saturating_sub(1);
    let cursor_clamped = cursor.min(max_idx);

    let display_rows: Vec<EditorAuthLineRow> = rows
        .iter()
        .map(|row| auth_display_row(row, &synthesized, &workspace_name))
        .collect();
    auth_lines(&display_rows, cursor_clamped, show_cursor)
}

#[must_use]
#[allow(clippy::type_complexity)]
pub fn auth_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> EditorTabContentGeometry {
    let rows = state.auth_flat_rows(config);
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let content_width = rows
        .iter()
        .map(|row| {
            let display_row = auth_display_row(row, &synthesized, &workspace_name);
            editor_auth_line_width(&display_row)
        })
        .max()
        .unwrap_or(0);
    EditorTabContentGeometry {
        content_width,
        content_height: rows.len(),
    }
}

fn editor_auth_source_display(
    synthesized: &jackin_config::AppConfig,
    workspace_name: &str,
    role: &str,
    kind: crate::tui::auth::AuthKind,
) -> AuthSourceDisplay {
    let mode = crate::tui::auth_config::resolve_panel_mode(synthesized, kind, workspace_name, role);
    let env_name = kind.required_env_var(mode);

    let value = env_name
        .and_then(|env_name| {
            crate::tui::auth_config::panel_auth_source_value(
                synthesized,
                workspace_name,
                role,
                env_name,
                kind,
            )
        })
        .map(|value| match value {
            jackin_core::EnvValue::OpRef(r) => AuthSourceValue::OpRefPath(r.path.clone()),
            jackin_core::EnvValue::Plain(s) => AuthSourceValue::Plain(s.clone()),
            jackin_core::EnvValue::Extended(e) => AuthSourceValue::Plain(e.value.clone()),
        });

    auth_source_display_for_required_env(
        env_name,
        value,
        crate::tui::components::auth_panel::mode_str(mode),
    )
}

#[must_use]
pub fn editor_auth_line_width(row: &EditorAuthLineRow) -> usize {
    match row {
        EditorAuthLineRow::AuthKind { label } => padded_width(&format!("  {label}")),
        EditorAuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let suffix = if *inherited { " (inherited)" } else { "" };
            padded_width(&format!(
                "  {:<AUTH_LABEL_COL_WIDTH$}{mode_label}{suffix}",
                "Mode"
            ))
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            auth_source_line_width("Source", display, 0)
        }
        EditorAuthLineRow::WorkspaceSourceFolder { display } => {
            source_folder_line_width("Source folder", display, 0)
        }
        EditorAuthLineRow::RoleHeader { role, .. } => {
            padded_width(&format!("\u{25bc} Role: {role}"))
        }
        EditorAuthLineRow::RoleMode { mode_label } => padded_width(&format!(
            "      {:<AUTH_LABEL_COL_WIDTH$}{mode_label}",
            "Mode"
        )),
        EditorAuthLineRow::RoleSource { display } => auth_source_line_width("Source", display, 6),
        EditorAuthLineRow::RoleSourceFolder { display } => {
            source_folder_line_width("Source folder", display, 6)
        }
        EditorAuthLineRow::AddSentinel { .. } => padded_width("  + Override for a role"),
        EditorAuthLineRow::Spacer => 0,
    }
}

fn render_auth_line(selected: bool, row: &EditorAuthLineRow) -> Line<'static> {
    let bold_white = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let dim_green = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let phosphor = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    match row {
        EditorAuthLineRow::AuthKind { label } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled(label.clone(), bold_white),
            ])
        }
        EditorAuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            let suffix = if *inherited { " (inherited)" } else { "" };
            Line::from(vec![
                Span::raw(cursor_col),
                Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), phosphor),
                Span::styled(suffix.to_owned(), dim_green),
            ])
        }
        EditorAuthLineRow::WorkspaceSource { display } => {
            render_auth_source_line("Source", display, 0, selected)
        }
        EditorAuthLineRow::WorkspaceSourceFolder { display } => {
            render_source_folder_line("Source folder", display, 0, selected)
        }
        EditorAuthLineRow::RoleHeader { role, expanded } => {
            let glyph = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
            Line::from(vec![
                Span::styled(glyph.to_owned(), disclosure_style()),
                Span::styled(format!(" Role: {role}"), disclosure_style()),
            ])
        }
        EditorAuthLineRow::RoleMode { mode_label } => Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
            Span::styled(mode_label.clone(), phosphor),
        ]),
        EditorAuthLineRow::RoleSource { display } => {
            render_auth_source_line("Source", display, 6, false)
        }
        EditorAuthLineRow::RoleSourceFolder { display } => {
            render_source_folder_line("Source folder", display, 6, false)
        }
        EditorAuthLineRow::AddSentinel { .. } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::styled(cursor_col, action_row_style(selected)),
                Span::styled("+ Override for a role", action_row_style(selected)),
            ])
        }
        EditorAuthLineRow::Spacer => Line::from(""),
    }
}

fn source_folder_line_width(
    label: &str,
    display: &AuthSourceFolderDisplay,
    indent: usize,
) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + text_width(&format!("{label:<label_width$}"));
    let value = source_folder_display_text(display);
    padded_width_cols(prefix_width + text_width(&value), gutter_width)
}

fn render_source_folder_line(
    label: &str,
    display: &AuthSourceFolderDisplay,
    indent: usize,
    selected: bool,
) -> Line<'static> {
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let prefix = if indent == 0 {
        cursor_col.to_owned()
    } else {
        " ".repeat(indent)
    };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let value = source_folder_display_text(display);
    Line::from(vec![
        Span::raw(prefix),
        Span::styled(
            format!("{label:<label_width$}"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)),
    ])
}

fn source_folder_display_text(display: &AuthSourceFolderDisplay) -> String {
    match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    }
}

fn auth_source_line_width(label: &str, display: &AuthSourceDisplay, indent: usize) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + text_width(&format!("{label:<label_width$}"));
    let value_width = match display {
        AuthSourceDisplay::NotRequired => text_width("not required"),
        AuthSourceDisplay::OpRefPath(path) => {
            text_width("[op] ")
                + crate::tui::op_breadcrumb::parse_path_breadcrumb(path).map_or_else(
                    || text_width("<unparseable path - re-pick>"),
                    |parts| crate::tui::op_breadcrumb::breadcrumb_display_width(&parts),
                )
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            text_width(&"\u{25cf}".repeat((*chars).clamp(1, 12)))
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => text_width(&format!("unset  ({env_name} for {mode_label})")),
    };
    padded_width_cols(prefix_width + value_width, gutter_width)
}

fn render_auth_source_line(
    label: &str,
    display: &AuthSourceDisplay,
    indent: usize,
    selected: bool,
) -> Line<'static> {
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let prefix = if indent == 0 {
        cursor_col.to_owned()
    } else {
        " ".repeat(indent)
    };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let mut spans = vec![
        Span::raw(prefix),
        Span::styled(
            format!("{label:<label_width$}"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled(
                "not required",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled(
                "[op] ",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
            crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => {
            spans.push(Span::styled(
                format!("unset  ({env_name} for {mode_label})"),
                Style::default().fg(jackin_tui::theme::DANGER_RED),
            ));
        }
    }

    Line::from(spans)
}

pub(crate) fn render_editor_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
    show_cursor: bool,
) -> Line<'static> {
    let selected = show_cursor && (row == cursor);
    let prefix = if selected { "\u{25b8} " } else { "  " };
    let label_style = if selected {
        Style::default()
            .fg(jackin_tui::theme::WHITE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(jackin_tui::theme::WHITE)
    };
    let value_style = if selected {
        Style::default()
            .fg(jackin_tui::theme::PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label:15}"), label_style),
        Span::styled(value.to_owned(), value_style),
    ])
}

pub fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

pub fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

pub fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
}

#[must_use]
pub fn tab_labels(active: EditorTab) -> Vec<(&'static str, bool)> {
    EditorTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub fn secrets_scope_label(scope: &SecretsScopeTag) -> &str {
    match scope {
        SecretsScopeTag::Workspace => "workspace",
        SecretsScopeTag::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn secrets_forbidden_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace env".to_owned(),
        SecretsScopeTag::Role(role) => format!("role {role}"),
    }
}

#[must_use]
pub fn secret_key_input_state<'a>(
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
    forbidden_keys: Vec<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state =
        jackin_tui::components::TextInputState::new_with_forbidden(label, initial, forbidden_keys);
    state.forbidden_label = secrets_forbidden_label(scope);
    state
}

#[must_use]
pub fn secret_key_input_state_from_pending<'a, R, V>(
    workspace_env: &std::collections::BTreeMap<String, V>,
    roles: &std::collections::BTreeMap<String, R>,
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
    role_env: impl Fn(&R) -> &std::collections::BTreeMap<String, V>,
) -> jackin_tui::components::TextInputState<'a> {
    secret_key_input_state(
        scope,
        label,
        initial,
        forbidden_secret_keys(workspace_env, roles, scope, role_env),
    )
}

/// Concrete adapter: render the editor screen with the standard footer.
///
/// Equivalent to the generic `render_editor_screen` but binds the concrete
/// `EditorState<'_>` and `editor_footer_items` so callers do not need to
/// construct the footer closure themselves.
pub fn render_editor_with_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::state::EditorState<'_>,
    config: &jackin_config::AppConfig,
    op_available: bool,
) {
    render_editor_screen(frame, area, state, config, |state, config, body| {
        crate::tui::components::footer_hints::editor_footer_items(state, config, op_available, body)
    });
}

#[cfg(test)]
mod tests;
