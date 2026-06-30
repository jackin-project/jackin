//! Editor screen view helpers.

use super::model::{AuthRow, EditorState, EditorTab, FieldFocus, SecretsRow};

#[allow(unused_imports)]
use crate::tui::components::editor_rows::{
    AUTH_LABEL_COL_WIDTH, AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind,
    AuthSourceValue, action_row_style, auth_source_display_for_required_env, disclosure_style,
    render_tab_strip,
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
    EditorRoleRow, editor_role_load_row_width, editor_role_row_width, editor_roles_status_width,
    role_lines, role_state_geometry, role_state_lines,
};

mod secrets_tab;
#[allow(unused_imports)]
pub(crate) use secrets_tab::{
    editor_secret_line_width, secret_key_line_width, secret_lines, secret_state_geometry,
    secret_state_lines,
};

mod auth_tab;
#[allow(unused_imports)]
pub(crate) use auth_tab::{
    EditorAuthLineRow, auth_lines, auth_state_geometry, auth_state_lines, editor_auth_line_width,
};

mod modals;
#[allow(unused_imports)]
pub(crate) use modals::{
    editor_header_title, editor_name_value, isolated_state_save_confirm_state,
    role_trust_confirm_state, secret_delete_confirm_prompt, secret_delete_confirm_state,
    secret_empty_key_label, secret_key_input_state, secret_key_input_state_from_pending,
    secret_new_key_after_picker_label, secret_new_key_label, secret_new_value_input_state,
    secret_scope_picker_state, secret_source_picker_state, secret_value_current_text,
    secret_value_input_state, secrets_forbidden_label, secrets_scope_label,
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
    auth_tab::auth_state_lines(state, config, editor_tab_content_focused(state))
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
        EditorTab::Secrets => secrets_tab::secret_state_geometry(state, area.width, |role| {
            config.roles.contains_key(role)
        }),
        EditorTab::Auth => auth_tab::auth_state_geometry(state, config),
    }
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
