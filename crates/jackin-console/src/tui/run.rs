// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared helpers for the host console event-loop shell.

use ratatui::layout::Rect;

use crate::tui::model::ConsoleManagerStageRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleScreenStage {
    List,
    Editor,
    Settings,
    CreatePrelude,
    ConfirmDelete,
    ConfirmInstancePurge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleChromeHover {
    DebugChip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MainScreenState {
    pub workspace_list: bool,
    pub list_modal_open: bool,
}

/// Bare `q` exits directly only from the plain workspace list. Other screens
/// or overlays use the quit confirmation flow.
#[must_use]
pub const fn is_main_screen(state: MainScreenState) -> bool {
    state.workspace_list && !state.list_modal_open
}

#[must_use]
pub const fn is_main_screen_for_route(
    route: ConsoleManagerStageRoute,
    list_modal_open: bool,
) -> bool {
    is_main_screen(MainScreenState {
        workspace_list: matches!(route, ConsoleManagerStageRoute::List),
        list_modal_open,
    })
}

#[must_use]
pub const fn console_screen_stage_for_route(route: ConsoleManagerStageRoute) -> ConsoleScreenStage {
    match route {
        ConsoleManagerStageRoute::List => ConsoleScreenStage::List,
        ConsoleManagerStageRoute::Editor => ConsoleScreenStage::Editor,
        ConsoleManagerStageRoute::Settings => ConsoleScreenStage::Settings,
        ConsoleManagerStageRoute::CreatePrelude => ConsoleScreenStage::CreatePrelude,
        ConsoleManagerStageRoute::ConfirmDelete => ConsoleScreenStage::ConfirmDelete,
        ConsoleManagerStageRoute::ConfirmInstancePurge => ConsoleScreenStage::ConfirmInstancePurge,
    }
}

/// Which diagnostics screen owns the visible console stage. Confirm dialogs
/// overlay the workspace list, so their telemetry remains attached to `List`.
#[must_use]
pub const fn diagnostics_screen_for_stage(stage: ConsoleScreenStage) -> jackin_diagnostics::Screen {
    match stage {
        ConsoleScreenStage::List
        | ConsoleScreenStage::ConfirmDelete
        | ConsoleScreenStage::ConfirmInstancePurge => jackin_diagnostics::Screen::List,
        ConsoleScreenStage::Editor => jackin_diagnostics::Screen::Editor,
        ConsoleScreenStage::Settings => jackin_diagnostics::Screen::Settings,
        ConsoleScreenStage::CreatePrelude => jackin_diagnostics::Screen::Create,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitInterceptState {
    pub on_main_screen: bool,
    pub consumes_letter_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenGenerateScopeLabel<'a> {
    Workspace(&'a str),
    WorkspaceRole { workspace: &'a str, role: &'a str },
    Global,
}

#[must_use]
pub fn token_generate_scope_label(scope: TokenGenerateScopeLabel<'_>) -> String {
    match scope {
        TokenGenerateScopeLabel::Workspace(name) => format!("workspace {name:?}"),
        TokenGenerateScopeLabel::WorkspaceRole { workspace, role } => {
            format!("workspace {workspace:?} role {role:?}")
        }
        TokenGenerateScopeLabel::Global => "global config".to_owned(),
    }
}

#[must_use]
pub fn token_generate_status_message(scope: TokenGenerateScopeLabel<'_>) -> String {
    let label = token_generate_scope_label(scope);
    format!(
        "\nGenerating Claude OAuth token for {label} -- complete the browser \
         sign-in, then paste the code below.\n"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetterInputModalKind {
    TextInput,
    FilterPicker,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LetterInputState {
    pub list_modal: Option<LetterInputModalKind>,
    pub editor_modal: Option<LetterInputModalKind>,
    pub create_prelude_modal: Option<LetterInputModalKind>,
    pub settings_mount_modal: Option<LetterInputModalKind>,
}

#[must_use]
pub const fn letter_input_state_for_route(
    route: ConsoleManagerStageRoute,
    list_modal: Option<LetterInputModalKind>,
    stage_modal: Option<LetterInputModalKind>,
) -> LetterInputState {
    let mut state = LetterInputState {
        list_modal,
        editor_modal: None,
        create_prelude_modal: None,
        settings_mount_modal: None,
    };
    match route {
        ConsoleManagerStageRoute::Editor => {
            state.editor_modal = stage_modal;
        }
        ConsoleManagerStageRoute::CreatePrelude => {
            state.create_prelude_modal = stage_modal;
        }
        ConsoleManagerStageRoute::Settings => {
            state.settings_mount_modal = stage_modal;
        }
        ConsoleManagerStageRoute::List
        | ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => {}
    }
    state
}

#[must_use]
pub const fn letter_input_modal_kind(
    text_input: bool,
    filter_picker: bool,
    modal_open: bool,
) -> Option<LetterInputModalKind> {
    if text_input {
        Some(LetterInputModalKind::TextInput)
    } else if filter_picker {
        Some(LetterInputModalKind::FilterPicker)
    } else if modal_open {
        Some(LetterInputModalKind::Other)
    } else {
        None
    }
}

/// Whether the active modal stack should receive bare letter keys.
///
/// The root console maps concrete modal variants into these generic facts.
/// Keeping the consumption policy here prevents the run loop from growing a
/// second copy of which component shapes type into filters or text inputs.
#[must_use]
pub const fn consumes_letter_input(state: LetterInputState) -> bool {
    modal_kind_consumes_letter_input(state.list_modal)
        || modal_kind_consumes_letter_input(state.editor_modal)
        || modal_kind_consumes_letter_input(state.create_prelude_modal)
        || modal_kind_consumes_letter_input(state.settings_mount_modal)
}

const fn modal_kind_consumes_letter_input(kind: Option<LetterInputModalKind>) -> bool {
    matches!(
        kind,
        Some(LetterInputModalKind::TextInput | LetterInputModalKind::FilterPicker)
    )
}

/// Whether a key should open the global exit confirmation.
///
/// Two triggers, matching every other jackin❯ surface:
/// * `Ctrl+Q` — the explicit quit chord. Always opens, regardless of screen or
///   focus: it is not a text character, so it never collides with typing.
/// * bare `q`/`Q` — a convenience trigger, but only off the main screen and
///   when no field is consuming letter input (otherwise it is just text).
///
/// The root console maps its stage/modal state into [`QuitInterceptState`].
/// Keeping the key policy here prevents the event loop from owning a parallel
/// interpretation of visible console focus.
#[must_use]
pub fn should_open_quit_confirm(
    key: crossterm::event::KeyEvent,
    state: QuitInterceptState,
) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    if !matches!(key.code, KeyCode::Char('q' | 'Q')) {
        return false;
    }
    let is_ctrl_q = key.modifiers.contains(KeyModifiers::CONTROL);
    let is_bare_q = (key.modifiers - KeyModifiers::SHIFT).is_empty()
        && !state.on_main_screen
        && !state.consumes_letter_input;
    is_ctrl_q || is_bare_q
}

#[must_use]
pub fn quit_confirm_state() -> crate::tui::components::ConfirmState {
    crate::tui::components::ConfirmState::new("Exit jackin❯?").with_focus_yes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitConfirmPlan {
    Exit,
    Dismiss,
    Continue,
}

#[must_use]
pub const fn quit_confirm_plan(outcome: jackin_tui::ModalOutcome<bool>) -> QuitConfirmPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(true) => QuitConfirmPlan::Exit,
        jackin_tui::ModalOutcome::Commit(false) | jackin_tui::ModalOutcome::Cancel => {
            QuitConfirmPlan::Dismiss
        }
        jackin_tui::ModalOutcome::Continue => QuitConfirmPlan::Continue,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModalBlockState {
    pub quit_confirm: bool,
    pub list_modal: bool,
    pub editor_modal: bool,
}

#[must_use]
pub const fn no_modal_blocks_base_surface(state: ModalBlockState) -> bool {
    !state.quit_confirm && !state.list_modal && !state.editor_modal
}

#[must_use]
pub const fn startup_error_was_dismissed(
    startup_error_pending: bool,
    list_modal_open: bool,
) -> bool {
    startup_error_pending && !list_modal_open
}

#[must_use]
pub const fn startup_error_modal_active(
    startup_error_pending: bool,
    list_modal_is_error_popup: bool,
) -> bool {
    startup_error_pending && list_modal_is_error_popup
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleClickStageFacts {
    List {
        list_modal_open: bool,
        workspace_list_target: bool,
    },
    Editor {
        modal_open: bool,
        tab_target: bool,
        mount_row_target: bool,
        auth_row_target: bool,
    },
    Settings {
        mounts_modal_open: bool,
        env_modal_open: bool,
        tab_target: bool,
        trust_target: bool,
    },
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsoleClickabilityFacts {
    pub pointer_supported: bool,
    pub file_browser_url_target: bool,
    pub container_info_copy_target: bool,
    pub stage: ConsoleClickStageFacts,
}

#[must_use]
pub const fn console_clickable_at(facts: ConsoleClickabilityFacts) -> bool {
    if !facts.pointer_supported {
        return false;
    }
    if facts.file_browser_url_target || facts.container_info_copy_target {
        return true;
    }
    match facts.stage {
        ConsoleClickStageFacts::List {
            list_modal_open,
            workspace_list_target,
        } => !list_modal_open && workspace_list_target,
        ConsoleClickStageFacts::Editor {
            modal_open,
            tab_target,
            mount_row_target,
            auth_row_target,
        } => !modal_open && (tab_target || mount_row_target || auth_row_target),
        ConsoleClickStageFacts::Settings {
            mounts_modal_open,
            env_modal_open,
            tab_target,
            trust_target,
        } => !mounts_modal_open && !env_modal_open && (tab_target || trust_target),
        ConsoleClickStageFacts::Other => false,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleModalMouseFacts {
    pub quit_confirm_open: bool,
    pub list_modal_open: bool,
    pub list_modal_container_info: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleModalMouseLayerFacts {
    pub quit_confirm_rect: Option<Rect>,
    pub list_modal_rect: Option<Rect>,
    pub list_modal_container_info: bool,
    pub startup_error_modal_active: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleModalMouseLayerPlan {
    pub consumed: bool,
    pub dismiss_quit_confirm: bool,
    pub dismiss_list_modal: bool,
}

#[must_use]
pub fn modal_mouse_layer_plan(
    mouse: crossterm::event::MouseEvent,
    facts: ConsoleModalMouseLayerFacts,
) -> ConsoleModalMouseLayerPlan {
    if let Some(rect) = facts.quit_confirm_rect {
        return ConsoleModalMouseLayerPlan {
            consumed: true,
            dismiss_quit_confirm: mouse_down_outside_rect(mouse, rect),
            dismiss_list_modal: false,
        };
    }

    let Some(rect) = facts.list_modal_rect else {
        return ConsoleModalMouseLayerPlan::default();
    };

    let consumed = modal_mouse_layer_consumes(
        mouse,
        ConsoleModalMouseFacts {
            quit_confirm_open: false,
            list_modal_open: true,
            list_modal_container_info: facts.list_modal_container_info,
        },
    );

    ConsoleModalMouseLayerPlan {
        consumed,
        dismiss_quit_confirm: false,
        dismiss_list_modal: !facts.startup_error_modal_active
            && mouse_down_outside_rect(mouse, rect),
    }
}

#[must_use]
pub const fn modal_mouse_layer_consumes(
    mouse: crossterm::event::MouseEvent,
    facts: ConsoleModalMouseFacts,
) -> bool {
    if facts.quit_confirm_open {
        return true;
    }
    if facts.list_modal_open {
        return !(mouse_is_wheel(mouse) && facts.list_modal_container_info);
    }
    false
}

#[must_use]
pub const fn debug_chip_activation_allowed(
    mouse: crossterm::event::MouseEvent,
    no_modal_open: bool,
    debug_chip_hovered: bool,
    active_run_present: bool,
) -> bool {
    matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_))
        && no_modal_open
        && debug_chip_hovered
        && active_run_present
}

#[must_use]
pub const fn console_pointer_shape(
    chrome_hovered: bool,
    base_clickable: bool,
) -> termrock::osc::PointerShape {
    if chrome_hovered || base_clickable {
        termrock::osc::PointerShape::Pointer
    } else {
        termrock::osc::PointerShape::Default
    }
}

const fn mouse_is_wheel(mouse: crossterm::event::MouseEvent) -> bool {
    matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::ScrollUp
            | crossterm::event::MouseEventKind::ScrollDown
            | crossterm::event::MouseEventKind::ScrollLeft
            | crossterm::event::MouseEventKind::ScrollRight
    )
}

fn mouse_down_outside_rect(mouse: crossterm::event::MouseEvent, rect: Rect) -> bool {
    matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_))
        && termrock::interaction::classify_click(rect, mouse.column, mouse.row)
            == termrock::interaction::ModalClickResult::OutsideDismiss
}

#[must_use]
pub fn should_dismiss_list_modal_for_outside_click(
    startup_error_modal_active: bool,
    modal_rect: Rect,
    column: u16,
    row: u16,
) -> bool {
    if startup_error_modal_active {
        return false;
    }

    termrock::interaction::classify_click(modal_rect, column, row)
        == termrock::interaction::ModalClickResult::OutsideDismiss
}

/// Split `area` into a main region and an optional 1-row debug bar at the
/// bottom.
#[must_use]
pub fn split_debug_area(area: Rect, debug_mode: bool) -> (Rect, Option<Rect>) {
    if !debug_mode || area.height < 3 {
        return (area, None);
    }
    // Reserve 2 rows: 1 blank spacer + 1 chip row.  The spacer separates the
    // hint bar from the debug chip (Defect 39 requirement: body → spacer →
    // hints → spacer → status/chip row).
    let main = Rect {
        height: area.height - 2,
        ..area
    };
    let bar = Rect {
        y: area.y + area.height - 2,
        height: 2,
        ..area
    };
    (main, Some(bar))
}

/// Return the 1-row rect within a `split_debug_area` bar where the chip
/// is actually rendered.  The top row of the 2-row bar is the blank spacer;
/// the chip lives in the bottom row.
#[must_use]
pub fn debug_chip_row(bar: Rect) -> Rect {
    if bar.height < 2 {
        return bar;
    }
    Rect {
        y: bar.y + bar.height - 1,
        height: 1,
        ..bar
    }
}

#[must_use]
pub fn debug_run_id_label(active_run_id: Option<&str>, env_run_id: Option<&str>) -> String {
    active_run_id
        .filter(|run_id| !run_id.is_empty())
        .or_else(|| env_run_id.filter(|run_id| !run_id.is_empty()))
        .unwrap_or_default()
        .to_owned()
}

#[must_use]
pub const fn should_debug_log_mouse(mouse: crossterm::event::MouseEvent) -> bool {
    // Skip only the high-frequency `Moved` (hover) flood. Clicks, drags, AND
    // scroll/wheel events must be logged — scroll events are exactly what a
    // "wheel does nothing" bug report needs, and filtering them out (as an
    // earlier version did) sent triage chasing a phantom "no wheel events".
    !matches!(mouse.kind, crossterm::event::MouseEventKind::Moved)
}

#[must_use]
pub fn quit_confirm_area(frame: Rect, confirm: &crate::tui::components::ConfirmState) -> Rect {
    // Structural exception: the root console quit prompt is outside `Modal`; it still uses shared confirm height and centered geometry.
    let width: u16 = 44.min(frame.width.saturating_sub(4));
    let height: u16 = confirm
        .required_height()
        .min(frame.height.saturating_sub(2));
    let x = frame.x + frame.width.saturating_sub(width) / 2;
    let y = frame.y + frame.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

// ── Concrete ConsoleState accessors ──────────────────────────────────────────
//
// These helpers extract facts from the concrete ConsoleState/ConsoleStage types
// that live in this crate. They avoid re-derivation in the root event loop.

pub const fn is_on_main_screen(state: &crate::tui::console::ConsoleState) -> bool {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    is_main_screen_for_route(ms.stage.route(), ms.list_modal.is_some())
}

pub const fn screen_of(state: &crate::tui::console::ConsoleState) -> jackin_diagnostics::Screen {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    diagnostics_screen_for_stage(console_screen_stage_for_route(ms.stage.route()))
}

pub const fn letter_input_state_for_console(
    state: &crate::tui::console::ConsoleState,
) -> LetterInputState {
    use crate::tui::state::ManagerStage;
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;

    let list_modal = match &ms.list_modal {
        Some(modal) => modal.letter_input_kind(),
        None => None,
    };
    let stage_modal = match &ms.stage {
        ManagerStage::Editor(editor) => match &editor.modal {
            Some(modal) => modal.letter_input_kind(),
            None => None,
        },
        ManagerStage::CreatePrelude(prelude) => match &prelude.modal {
            Some(modal) => modal.letter_input_kind(),
            None => None,
        },
        ManagerStage::Settings(settings) => match &settings.mounts.modal {
            Some(modal) => modal.letter_input_kind(),
            None => None,
        },
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => None,
    };

    letter_input_state_for_route(ms.stage.route(), list_modal, stage_modal)
}

pub const fn quit_intercept_state_for_console(
    state: &crate::tui::console::ConsoleState,
) -> QuitInterceptState {
    QuitInterceptState {
        on_main_screen: is_on_main_screen(state),
        consumes_letter_input: consumes_letter_input(letter_input_state_for_console(state)),
    }
}

pub fn no_modal_open(state: &crate::tui::console::ConsoleState) -> bool {
    state.base_surface_unblocked()
}

pub const fn startup_error_dismissed(
    state: &crate::tui::console::ConsoleState,
    startup_error_pending: bool,
) -> bool {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    startup_error_was_dismissed(startup_error_pending, ms.list_modal.is_some())
}

pub fn startup_error_modal_active_for_console(
    state: &crate::tui::console::ConsoleState,
    startup_error_pending: bool,
) -> bool {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    startup_error_modal_active(
        startup_error_pending,
        matches!(
            ms.list_modal,
            Some(crate::tui::state::Modal::ErrorPopup { .. })
        ),
    )
}

pub fn token_generate_scope_label_for_console(
    req: &crate::tui::state::PendingTokenGenerate,
) -> TokenGenerateScopeLabel<'_> {
    use jackin_env::TokenSetupScope;
    match &req.scope {
        TokenSetupScope::Workspace(name) => TokenGenerateScopeLabel::Workspace(name),
        TokenSetupScope::WorkspaceRole { workspace, role } => {
            TokenGenerateScopeLabel::WorkspaceRole { workspace, role }
        }
        TokenSetupScope::Global => TokenGenerateScopeLabel::Global,
    }
}

#[cfg(test)]
mod tests;
