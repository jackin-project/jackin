//! Shared helpers for the host console event-loop shell.

use ratatui::layout::Rect;

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
        TokenGenerateScopeLabel::Global => "global config".to_string(),
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

/// Whether the bare `q`/`Q` key should open the global exit confirmation.
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

    matches!(key.code, KeyCode::Char('q' | 'Q'))
        && (key.modifiers - KeyModifiers::SHIFT).is_empty()
        && !state.on_main_screen
        && !state.consumes_letter_input
}

#[must_use]
pub fn quit_confirm_state() -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new("Exit jackin'?")
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
pub fn debug_run_id_label(run_id: Option<&str>) -> String {
    run_id.unwrap_or_default().to_string()
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
pub fn quit_confirm_area(frame: Rect, confirm: &jackin_tui::components::ConfirmState) -> Rect {
    let width: u16 = 44.min(frame.width.saturating_sub(4));
    let height: u16 = jackin_tui::components::confirm_required_height(confirm)
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

#[cfg(test)]
mod tests;
