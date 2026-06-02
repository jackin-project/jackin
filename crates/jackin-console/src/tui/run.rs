//! Shared helpers for the host console event-loop shell.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitInterceptState {
    pub on_main_screen: bool,
    pub consumes_letter_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenGenerateScopeLabel<'a> {
    Workspace(&'a str),
    WorkspaceRole {
        workspace: &'a str,
        role: &'a str,
    },
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
    if !debug_mode || area.height < 2 {
        return (area, None);
    }
    let main = Rect {
        height: area.height - 1,
        ..area
    };
    let bar = Rect {
        y: area.y + area.height - 1,
        height: 1,
        ..area
    };
    (main, Some(bar))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn quit_intercept_opens_off_main_for_bare_q() {
        let state = QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: false,
        };

        assert!(should_open_quit_confirm(
            key(KeyCode::Char('q'), KeyModifiers::NONE),
            state,
        ));
        assert!(should_open_quit_confirm(
            key(KeyCode::Char('Q'), KeyModifiers::SHIFT),
            state,
        ));
    }

    #[test]
    fn quit_intercept_ignores_main_text_input_and_modified_keys() {
        assert!(!should_open_quit_confirm(
            key(KeyCode::Char('q'), KeyModifiers::NONE),
            QuitInterceptState {
                on_main_screen: true,
                consumes_letter_input: false,
            },
        ));
        assert!(!should_open_quit_confirm(
            key(KeyCode::Char('q'), KeyModifiers::NONE),
            QuitInterceptState {
                on_main_screen: false,
                consumes_letter_input: true,
            },
        ));
        assert!(!should_open_quit_confirm(
            key(KeyCode::Char('q'), KeyModifiers::CONTROL),
            QuitInterceptState {
                on_main_screen: false,
                consumes_letter_input: false,
            },
        ));
    }

    #[test]
    fn letter_input_state_detects_text_and_filter_modals() {
        assert!(consumes_letter_input(LetterInputState {
            editor_modal: Some(LetterInputModalKind::TextInput),
            ..LetterInputState::default()
        }));
        assert!(consumes_letter_input(LetterInputState {
            list_modal: Some(LetterInputModalKind::FilterPicker),
            ..LetterInputState::default()
        }));
        assert!(!consumes_letter_input(LetterInputState {
            settings_mount_modal: Some(LetterInputModalKind::Other),
            ..LetterInputState::default()
        }));
        assert!(!consumes_letter_input(LetterInputState::default()));
    }

    #[test]
    fn token_generate_status_message_names_target_scope() {
        assert_eq!(
            token_generate_scope_label(TokenGenerateScopeLabel::Workspace("proj")),
            "workspace \"proj\""
        );
        assert_eq!(
            token_generate_scope_label(TokenGenerateScopeLabel::WorkspaceRole {
                workspace: "proj",
                role: "ops",
            }),
            "workspace \"proj\" role \"ops\""
        );
        assert_eq!(
            token_generate_status_message(TokenGenerateScopeLabel::Global),
            "\nGenerating Claude OAuth token for global config -- complete the browser sign-in, then paste the code below.\n"
        );
    }

    #[test]
    fn debug_bar_chip_area_matches_right_aligned_chip_width() {
        let area = Rect {
            x: 2,
            y: 5,
            width: 20,
            height: 1,
        };

        assert_eq!(
            debug_bar_chip_area(area, "run", None),
            Rect {
                x: 17,
                y: 5,
                width: 5,
                height: 1,
            }
        );
        assert_eq!(
            debug_bar_chip_area(area, "run", Some("inst")),
            Rect {
                x: 12,
                y: 5,
                width: 10,
                height: 1,
            }
        );
    }
}

/// Render the 1-row debug status bar.
///
/// When `instance_id` is provided, shows `run_id:instance_id` as a single
/// danger chip right-aligned on a white bar. The combined chip is clickable
/// in the root event loop.
pub fn render_debug_bar(frame: &mut Frame, area: Rect, run_id: &str, instance_id: Option<&str>) {
    let chip_text =
        instance_id.map_or_else(|| format!(" {run_id} "), |iid| format!(" {run_id}:{iid} "));
    let [left_area, chip_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(debug_bar_chip_width(
            run_id,
            instance_id,
        ))])
        .areas(area);

    let white_bg = Style::default()
        .bg(jackin_tui::theme::WHITE)
        .fg(jackin_tui::theme::PHOSPHOR_DARK);
    let chip_style = Style::default()
        .bg(jackin_tui::theme::DANGER_RED)
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::raw("")])).style(white_bg),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(chip_text, chip_style)])),
        chip_area,
    );
}

#[must_use]
pub fn debug_bar_chip_area(area: Rect, run_id: &str, instance_id: Option<&str>) -> Rect {
    let chip_width = debug_bar_chip_width(run_id, instance_id);
    Rect {
        x: area.x + area.width.saturating_sub(chip_width),
        y: area.y,
        width: chip_width.min(area.width),
        height: 1,
    }
}

fn debug_bar_chip_width(run_id: &str, instance_id: Option<&str>) -> u16 {
    let content_width = run_id.chars().count()
        + instance_id.map_or(0, |instance_id| instance_id.chars().count() + 1);
    (content_width + 2) as u16
}

#[must_use]
pub const fn should_debug_log_mouse(mouse: crossterm::event::MouseEvent) -> bool {
    !matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::ScrollDown
            | crossterm::event::MouseEventKind::ScrollUp
            | crossterm::event::MouseEventKind::ScrollLeft
            | crossterm::event::MouseEventKind::ScrollRight
    )
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
