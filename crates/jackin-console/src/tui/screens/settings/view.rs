//! Settings screen view helpers.

use super::model::AuthFormFocus;
use super::model::AuthFormTarget;
use super::model::GlobalMountConfirm;
use super::model::GlobalMountModal;
use super::model::GlobalMountTextTarget;
use super::model::GlobalMountsState;
use super::model::SettingsAuthModal;
use super::model::SettingsAuthRow;
use super::model::SettingsAuthState;
use super::model::SettingsEnvConfig;
use super::model::SettingsEnvModal;
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
use jackin_tui::HintSpan;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::collections::BTreeMap;

use crate::tui::components::editor_rows::{
    AUTH_LABEL_COL_WIDTH, AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind,
    AuthSourceValue, SecretValueDisplay, action_row_style, auth_source_display, disclosure_style,
    render_secret_key_line, render_tab_strip,
};
use crate::tui::components::footer_hints::{
    SettingsContextFooterMode, SettingsScreenFooterFacts, content_footer_items,
    settings_contextual_row_footer_items, settings_save_footer_label, settings_screen_footer_items,
    tab_bar_footer_items,
};
use crate::tui::components::mount_rows::MOUNT_MODE_COL_WIDTH;
use crate::tui::input::settings_auth_can_generate_token;
use crate::tui::mount_display::{
    MountDisplayRow, format_config_mount_rows_with_cache, mount_path_width,
};
use crate::tui::view::{
    effective_footer_height, measured_footer_height, render_footer, render_header,
};

// Structural exception: settings rows are form/table rows with labels, values,
// disclosures, masked secrets, and action sentinels, so they cannot use the
// flat picker renderer even though they share its focus-gated cursor contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAuthLineRow {
    Kind { label: String },
    Mode { mode_label: String },
    Source { display: AuthSourceDisplay },
    SourceFolder { display: AuthSourceFolderDisplay },
    Spacer,
}

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
    reason = "tracked in codebase-health-enforcement"
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame, area, lines, 0, 0, focused, None,
    );
}

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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
) -> jackin_tui::components::ScrollAxes {
    let content = crate::tui::screens::settings::update::trust_content_width(&state.trust);
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.trust.pending.is_empty(),
        content,
        body_area,
    )
}

#[allow(clippy::type_complexity)]
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
) -> jackin_tui::components::ScrollAxes {
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

#[allow(clippy::type_complexity)]
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

pub fn render_global_mount_modal<R, M>(
    frame: &mut Frame<'_>,
    modal: &GlobalMountModal<
        jackin_tui::components::TextInputState<'_>,
        crate::tui::components::file_browser::FileBrowserState,
        crate::tui::components::mount_dst_choice::MountDstChoiceState,
        crate::tui::components::scope_picker::ScopePickerState,
        crate::tui::components::role_picker::RolePickerState<R>,
        jackin_tui::components::ConfirmState,
        crate::tui::components::confirm_save::ConfirmSaveState<M>,
    >,
) where
    R: crate::tui::components::role_picker::RoleChoice,
    M: Clone,
{
    let area =
        crate::tui::components::modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        GlobalMountModal::Text { state, .. } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        GlobalMountModal::FileBrowser { state } => {
            crate::tui::components::file_browser::render(frame, area, state);
        }
        GlobalMountModal::MountDstChoice { state } => {
            crate::tui::components::mount_dst_choice::render(frame, area, state);
        }
        GlobalMountModal::ScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, area, state);
        }
        GlobalMountModal::RolePicker { state } => {
            crate::tui::components::role_picker::render(frame, area, state);
        }
        GlobalMountModal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
        GlobalMountModal::PreviewSave { state } => {
            crate::tui::components::confirm_save::render(frame, area, state);
        }
    }
}

pub fn render_settings_env_modal<O, R>(
    frame: &mut Frame<'_>,
    modal: &SettingsEnvModal<
        jackin_tui::components::TextInputState<'_>,
        crate::tui::components::source_picker::SourcePickerState,
        O,
        crate::tui::components::role_picker::RolePickerState<R>,
        crate::tui::components::scope_picker::ScopePickerState,
        jackin_tui::components::ConfirmState,
    >,
) where
    O: crate::tui::components::op_picker::OpPickerRenderState
        + crate::tui::components::modal_rects::ModalOpPickerState,
    R: crate::tui::components::role_picker::RoleChoice,
{
    let area =
        crate::tui::components::modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsEnvModal::Text { state, .. } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsEnvModal::SourcePicker { state } => {
            crate::tui::components::source_picker::render(frame, area, state);
        }
        SettingsEnvModal::OpPicker { state } => {
            crate::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        SettingsEnvModal::RolePicker { state } => {
            crate::tui::components::role_picker::render(frame, area, state);
        }
        SettingsEnvModal::ScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsEnvModal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
    }
}

pub fn render_settings_auth_modal<O, K, V>(
    frame: &mut Frame<'_>,
    modal: &SettingsAuthModal<
        jackin_tui::components::TextInputState<'_>,
        crate::tui::components::source_picker::SourcePickerState,
        O,
        crate::tui::components::file_browser::FileBrowserState,
        AuthFormTarget<K>,
        crate::tui::components::auth_panel::AuthForm<V>,
        AuthFormFocus,
    >,
) where
    O: crate::tui::components::op_picker::OpPickerRenderState
        + crate::tui::components::modal_rects::ModalOpPickerState,
    V: crate::tui::components::auth_panel::AuthCredential,
{
    let area =
        crate::tui::components::modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            crate::tui::components::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsAuthModal::SourcePicker { state } => {
            crate::tui::components::source_picker::render(frame, area, state);
        }
        SettingsAuthModal::TextInput { state } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsAuthModal::SourceFolderPicker { state } => {
            crate::tui::components::file_browser::render(frame, area, state);
        }
        SettingsAuthModal::OpPicker { state } => {
            crate::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
    }
}

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::type_complexity)]
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
pub const fn settings_header_title() -> &'static str {
    "settings"
}

#[must_use]
pub fn tab_labels(active: SettingsTab) -> Vec<(&'static str, bool)> {
    SettingsTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub const fn global_mount_confirm_prompt(action: GlobalMountConfirm) -> &'static str {
    match action {
        GlobalMountConfirm::Save => "Save settings to ~/.config/jackin/config.toml?",
        GlobalMountConfirm::Sensitive => "Sensitive global mount path detected. Save anyway?",
        GlobalMountConfirm::Remove => "Remove selected global mount?",
        GlobalMountConfirm::Discard => "Discard unsaved global mount changes?",
    }
}

#[must_use]
pub fn global_mount_confirm_state(
    action: GlobalMountConfirm,
) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(global_mount_confirm_prompt(action))
}

#[must_use]
pub fn global_mount_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::with_title(
        " Which agent role do you want to add? ",
    )
}

#[must_use]
pub fn global_mount_text_input_state<'a>(
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new(label, initial)
}

#[must_use]
pub fn global_mount_scope_text_value(scope: Option<&str>) -> String {
    scope.unwrap_or_default().to_owned()
}

#[must_use]
pub fn global_mount_edit_text_initial(
    row: &jackin_config::GlobalMountRow,
    target: &GlobalMountTextTarget,
) -> Option<String> {
    match target {
        GlobalMountTextTarget::Rename => Some(row.name.clone()),
        GlobalMountTextTarget::Source => Some(row.mount.src.clone()),
        GlobalMountTextTarget::Destination => Some(row.mount.dst.clone()),
        GlobalMountTextTarget::Scope => Some(global_mount_scope_text_value(row.scope.as_deref())),
        GlobalMountTextTarget::AddScope
        | GlobalMountTextTarget::AddName
        | GlobalMountTextTarget::AddSource
        | GlobalMountTextTarget::AddDestination => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalMountEditTextPlan {
    pub target: GlobalMountTextTarget,
    pub label: &'static str,
    pub initial: String,
}

#[must_use]
pub fn global_mount_selected_edit_text_plan(
    rows: &[jackin_config::GlobalMountRow],
    selected: usize,
    target: GlobalMountTextTarget,
) -> Option<GlobalMountEditTextPlan> {
    let row = rows.get(selected)?;
    let initial = global_mount_edit_text_initial(row, &target)?;
    let label = global_mount_text_target_label(&target)?;
    Some(GlobalMountEditTextPlan {
        target,
        label,
        initial,
    })
}

#[must_use]
pub const fn global_mount_text_target_label(
    target: &GlobalMountTextTarget,
) -> Option<&'static str> {
    match target {
        GlobalMountTextTarget::AddScope => Some("Scope (empty = global)"),
        GlobalMountTextTarget::AddName => Some("Mount name"),
        GlobalMountTextTarget::AddSource => Some("Source"),
        GlobalMountTextTarget::AddDestination => Some("Destination"),
        GlobalMountTextTarget::Source => Some("Source"),
        GlobalMountTextTarget::Destination => Some("Destination"),
        GlobalMountTextTarget::Scope => Some("Scope (empty = global)"),
        GlobalMountTextTarget::Rename => Some("Rename mount"),
    }
}

#[must_use]
pub fn settings_env_text_input_state<'a>(
    target: &SettingsEnvTextTarget,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    if matches!(target, SettingsEnvTextTarget::EnvValue { .. }) {
        jackin_tui::components::TextInputState::new_allow_empty(label, initial)
    } else {
        jackin_tui::components::TextInputState::new(label, initial)
    }
}

#[must_use]
pub fn settings_env_value_text_label(key: &str) -> String {
    format!("Edit {key}")
}

#[must_use]
pub fn settings_env_value_current_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvValueEditTextPlan {
    pub target: SettingsEnvTextTarget,
    pub label: String,
    pub current: String,
}

#[must_use]
pub fn settings_env_value_edit_text_plan(
    pending: &SettingsEnvConfig<jackin_core::EnvValue>,
    scope: SettingsEnvScope,
    key: String,
) -> SettingsEnvValueEditTextPlan {
    let value = super::update::settings_env_value(pending, &scope, &key);
    let current =
        settings_env_value_current_text(value.map(jackin_core::EnvValue::as_persisted_str));
    SettingsEnvValueEditTextPlan {
        target: SettingsEnvTextTarget::EnvValue {
            scope,
            key: key.clone(),
        },
        label: settings_env_value_text_label(&key),
        current,
    }
}

#[must_use]
pub fn settings_env_plain_value_text_plan(
    scope: SettingsEnvScope,
    key: String,
) -> SettingsEnvValueEditTextPlan {
    SettingsEnvValueEditTextPlan {
        target: SettingsEnvTextTarget::EnvValue {
            scope,
            key: key.clone(),
        },
        label: settings_env_value_text_label(&key),
        current: String::new(),
    }
}

#[must_use]
pub fn settings_env_source_picker_state(
    key: impl Into<String>,
) -> crate::tui::components::source_picker::SourcePickerState {
    crate::tui::components::source_picker::SourcePickerState::new(key.into(), true)
}

#[must_use]
pub fn settings_env_scope_picker_state() -> crate::tui::components::scope_picker::ScopePickerState {
    crate::tui::components::scope_picker::ScopePickerState::new()
}

#[must_use]
pub fn settings_env_delete_confirm_prompt(key: &str) -> String {
    format!("Delete environment variable {key}?")
}

#[must_use]
pub fn settings_env_delete_confirm_state(key: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(settings_env_delete_confirm_prompt(key))
}

#[must_use]
pub fn env_scope_label(scope: &SettingsEnvScope) -> &str {
    match scope {
        SettingsEnvScope::Global => "global",
        SettingsEnvScope::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn settings_env_new_key_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "New global environment key".to_owned(),
        SettingsEnvScope::Role(role) => format!("New {role} environment key"),
    }
}

#[must_use]
pub fn settings_env_new_key_after_picker_label(scope: &SettingsEnvScope) -> String {
    format!("New environment key for {}", env_scope_label(scope))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvKeyTextPlan {
    pub scope: SettingsEnvScope,
    pub target: SettingsEnvTextTarget,
    pub label: String,
}

#[must_use]
pub fn settings_env_key_text_plan(
    scope: SettingsEnvScope,
    label: impl Into<String>,
) -> SettingsEnvKeyTextPlan {
    SettingsEnvKeyTextPlan {
        target: SettingsEnvTextTarget::EnvKey {
            scope: scope.clone(),
        },
        scope,
        label: label.into(),
    }
}

#[must_use]
pub fn settings_env_new_key_text_plan(scope: SettingsEnvScope) -> SettingsEnvKeyTextPlan {
    let label = settings_env_new_key_label(&scope);
    settings_env_key_text_plan(scope, label)
}

#[must_use]
pub fn settings_env_new_key_after_picker_text_plan(
    scope: SettingsEnvScope,
) -> SettingsEnvKeyTextPlan {
    let label = settings_env_new_key_after_picker_label(&scope);
    settings_env_key_text_plan(scope, label)
}

#[must_use]
pub fn settings_env_empty_key_text_plan(scope: SettingsEnvScope) -> SettingsEnvKeyTextPlan {
    settings_env_key_text_plan(scope, settings_env_empty_key_label())
}

#[must_use]
pub fn settings_env_empty_key_label() -> &'static str {
    "Key cannot be empty"
}

#[must_use]
pub fn settings_env_empty_key_error_message() -> &'static str {
    "Env key cannot be empty."
}

#[must_use]
pub fn global_mount_name_empty_message() -> &'static str {
    "Mount name cannot be empty."
}

#[must_use]
pub fn global_mount_gone_message() -> &'static str {
    "Mount no longer exists; selection was cleared."
}

#[must_use]
pub fn global_mount_add_draft_lost_message() -> &'static str {
    "Add-mount draft was lost; press 'a' to start over."
}

#[must_use]
pub fn global_mount_destination_empty_message() -> &'static str {
    "Mount destination cannot be empty."
}

#[must_use]
pub fn global_mount_no_github_url_message() -> &'static str {
    "no GitHub URL for this mount"
}

#[must_use]
pub fn settings_no_registered_roles_error_message() -> &'static str {
    "No registered roles available."
}

#[must_use]
pub fn settings_sensitive_paths_not_confirmed_message() -> &'static str {
    "Save aborted: sensitive paths not confirmed."
}

#[must_use]
pub fn settings_error_popup_title() -> &'static str {
    "Settings error"
}

#[must_use]
pub fn settings_auth_op_read_failed_message(error: impl std::fmt::Display) -> String {
    format!("1Password read failed: {error}")
}

#[must_use]
pub fn env_forbidden_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "global env".to_owned(),
        SettingsEnvScope::Role(role) => format!("role {role}"),
    }
}

#[must_use]
pub fn settings_env_key_input_state<'a, V>(
    pending: &SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    let mut state = jackin_tui::components::TextInputState::new_with_forbidden(
        label,
        initial,
        forbidden_settings_env_keys(pending, scope),
    );
    state.forbidden_label = env_forbidden_label(scope);
    state
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
    let label_bold = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let label_normal = Style::default().fg(jackin_tui::theme::WHITE);
    let value_bold = Style::default()
        .fg(jackin_tui::theme::PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let value_normal = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    let rows: [(usize, &str, bool); 2] = [
        (0, "Co-author trailer", pending_coauthor_trailer),
        (1, "DCO sign-off", pending_dco),
    ];

    rows.iter()
        .map(|(i, label, pending)| {
            let selected = show_cursor && (selected_row == *i);
            let prefix = if selected { "\u{25b8} " } else { "  " };
            let ls = if selected { label_bold } else { label_normal };
            let vs = if selected { value_bold } else { value_normal };
            let value = if *pending { "enabled" } else { "disabled" };
            Line::from(vec![
                Span::styled(prefix, ls),
                Span::styled(format!("{label:<26}"), ls),
                Span::styled(value, vs),
            ])
        })
        .collect()
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
        Style::default().fg(jackin_tui::theme::WHITE),
    ))];
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (selected_row == i);
        let mut style = if selected {
            Style::default()
                .fg(jackin_tui::theme::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        };
        if !selected && hovered_row == Some(i) {
            style = style.bg(jackin_tui::theme::TAB_BG_INACTIVE_HOVER);
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
    let mut lines = Vec::with_capacity(rows.len());
    let label_width = 22;
    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (selected_row == i);
        let cursor_col = if selected { "\u{25b8} " } else { "  " };
        match row {
            SettingsEnvRow::Key { scope, key } => {
                let Some(value) = value_for(scope, key) else {
                    continue;
                };
                lines.push(render_secret_key_line(
                    selected,
                    cursor_col,
                    key,
                    value,
                    !is_unmasked(scope, key),
                    area_width,
                    label_width,
                ));
            }
            SettingsEnvRow::GlobalAddSentinel => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add environment variable"),
                    action_row_style(selected),
                )));
            }
            SettingsEnvRow::RoleHeader { role, expanded } => {
                let arrow = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
                lines.push(Line::from(vec![
                    Span::raw(cursor_col.to_owned()),
                    Span::styled(arrow.to_owned(), disclosure_style()),
                    Span::styled(
                        format!(" Role: {role}  ({} vars)", role_var_count(role)),
                        disclosure_style(),
                    ),
                ]));
            }
            SettingsEnvRow::RoleAddSentinel(role) => {
                lines.push(Line::from(Span::styled(
                    format!("{cursor_col}+ Add {role} environment variable"),
                    action_row_style(selected),
                )));
            }
            SettingsEnvRow::SectionSpacer => lines.push(Line::from("")),
        }
    }
    lines
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
    rows: &[SettingsAuthLineRow],
    selected_row: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = show_cursor && (selected_row == i);
            render_auth_line(row, selected)
        })
        .collect()
}

#[must_use]
pub fn auth_state_lines<AuthModal, EnvModal, PendingOpCommit>(
    auth: &SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
    env: &SettingsEnvState<jackin_core::EnvValue, EnvModal>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let Some(kind) = auth.selected_kind else {
        let rows: Vec<SettingsAuthLineRow> = auth
            .pending
            .iter()
            .map(|row| SettingsAuthLineRow::Kind {
                label: row.kind.label().to_owned(),
            })
            .collect();
        return auth_lines(&rows, auth.selected, show_cursor);
    };

    let Some(row) = auth.pending.iter().find(|row| row.kind == kind) else {
        return Vec::new();
    };

    let mut rows = vec![SettingsAuthLineRow::Mode {
        mode_label: crate::tui::components::auth_panel::mode_str(row.mode).to_owned(),
    }];
    if let Some(env_name) = kind.required_env_var(row.mode) {
        rows.push(SettingsAuthLineRow::Source {
            display: settings_auth_source_display(auth, env, kind, row.mode, env_name),
        });
    }
    if crate::tui::auth::auth_mode_supports_source_folder(kind, row.mode) {
        rows.push(SettingsAuthLineRow::SourceFolder {
            display: crate::tui::auth_config::settings_source_folder_display(row),
        });
    }
    rows.push(SettingsAuthLineRow::Spacer);
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

fn render_auth_line(row: &SettingsAuthLineRow, selected: bool) -> Line<'static> {
    let bold_white = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let phosphor = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);

    match row {
        SettingsAuthLineRow::Kind { label } => {
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(Span::styled(format!("{cursor_col}{label}"), bold_white))
        }
        SettingsAuthLineRow::Mode { mode_label } => {
            let mode_style = if selected {
                phosphor.add_modifier(Modifier::BOLD)
            } else {
                phosphor
            };
            let cursor_col = if selected { "\u{25b8} " } else { "  " };
            Line::from(vec![
                Span::styled(cursor_col, mode_style),
                Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), mode_style),
            ])
        }
        SettingsAuthLineRow::Source { display } => render_auth_source_line(display, selected),
        SettingsAuthLineRow::SourceFolder { display } => {
            render_auth_source_folder_line(display, selected)
        }
        SettingsAuthLineRow::Spacer => Line::from(""),
    }
}

fn render_auth_source_folder_line(
    display: &AuthSourceFolderDisplay,
    selected: bool,
) -> Line<'static> {
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let source_style = if selected {
        dim.add_modifier(Modifier::BOLD)
    } else {
        dim
    };
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let value = match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    };
    Line::from(vec![
        Span::styled(cursor_col, source_style),
        Span::styled(
            format!("{:<AUTH_LABEL_COL_WIDTH$}", "Source folder"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, source_style),
    ])
}

fn render_auth_source_line(display: &AuthSourceDisplay, selected: bool) -> Line<'static> {
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let source_style = if selected {
        dim.add_modifier(Modifier::BOLD)
    } else {
        dim
    };
    let cursor_col = if selected { "\u{25b8} " } else { "  " };
    let mut spans = vec![
        Span::styled(cursor_col, source_style),
        Span::styled(
            format!("{:<AUTH_LABEL_COL_WIDTH$}", "Source"),
            Style::default()
                .fg(jackin_tui::theme::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled("not required", source_style));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled("[op] ", source_style));
            crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                source_style,
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

#[must_use]
pub fn global_mount_lines(
    rows: &[MountDisplayRow],
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if !rows.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {path:<path_w$}  {mode:<MOUNT_MODE_COL_WIDTH$}  Type",
                path = "Destination",
                mode = "Mode"
            ),
            Style::default().fg(jackin_tui::theme::WHITE),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let is_selected = selected == Some(i);
        let prefix = if is_selected { "\u{25b8} " } else { "  " };
        let base_style = if is_selected {
            Style::default()
                .fg(jackin_tui::theme::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(jackin_tui::theme::PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{:<path_w$}  ", row.destination),
                base_style,
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(row.kind.clone(), dim_style),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
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
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        content_width,
        jackin_tui::components::scrollable_panel::viewport_width(areas.body),
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
            .map(|modal| modal.footer_items(settings_auth_can_generate_token(&state.auth))),
        env_modal_items: state.env.modal.as_ref().map(SettingsEnvModal::footer_items),
        mounts_modal_items: state
            .mounts
            .modal
            .as_ref()
            .map(GlobalMountModal::footer_items),
        screen_items: settings_footer_items(state, op_available, body_area),
    })
}

#[cfg(test)]
mod tests;
