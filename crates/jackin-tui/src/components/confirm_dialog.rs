// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Y/N confirmation modal with keyboard focus.
//!
//! Y / N / Esc return distinct outcomes; case-insensitive.
//! Tab / left / right / h/l cycle focus between Yes and No.
//! Enter commits the focused button.

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    HintSpan, ModalOutcome,
    components::ButtonFocus,
    keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility},
    theme::{PHOSPHOR_GREEN, WARNING_YELLOW},
};

use super::button_strip::{ButtonStrip, ButtonStripItem};
use super::dialog_layout::{DialogBorder, dialog_inner_chunks, render_dialog_shell};

/// Actions the confirmation dialog can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    /// `Y`/`y` — always commits Yes regardless of button focus.
    Yes,
    /// `N`/`n` — always commits No regardless of button focus.
    No,
    /// `Esc` — cancel the dialog (caller-interpreted as No / dismiss).
    Cancel,
    /// `Tab`/`BackTab` — toggle focus between Yes and No.
    ToggleFocus,
    /// `Enter` — commit whichever button is currently focused.
    CommitFocused,
}

const CONFIRM_BINDINGS: &[KeyBinding<ConfirmAction>] = &[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: ConfirmAction::CommitFocused,
        hint: Some("confirm"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('y')),
            KeyChord::plain(LogicalKey::Char('Y')),
        ],
        action: ConfirmAction::Yes,
        hint: Some("yes"),
        visibility: Visibility::Shown,
        glyph: Some("Y"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('n')),
            KeyChord::plain(LogicalKey::Char('N')),
        ],
        action: ConfirmAction::No,
        hint: Some("no"),
        visibility: Visibility::Shown,
        // Combined glyph — Esc is a hidden alias below, shown here in the label.
        glyph: Some("N/Esc"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::ctrl(LogicalKey::Char('c')),
            KeyChord::ctrl(LogicalKey::Char('q')),
        ],
        action: ConfirmAction::Cancel,
        hint: None,
        // Esc/Ctrl+C/Ctrl+Q advertised via the combined "N/Esc" glyph on the No binding.
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Tab),
            KeyChord::plain(LogicalKey::BackTab),
        ],
        action: ConfirmAction::ToggleFocus,
        hint: Some("focus"),
        visibility: Visibility::Shown,
        glyph: Some("\u{21e5}"), // ⇥
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Left),
            KeyChord::plain(LogicalKey::Right),
            KeyChord::plain(LogicalKey::Char('h')),
            KeyChord::plain(LogicalKey::Char('l')),
        ],
        action: ConfirmAction::ToggleFocus,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
];

/// Single-source-of-truth keymap for [`ConfirmState`].
///
/// `hint_spans()` produces `↵ confirm   Y yes   N/Esc no   ⇥ focus`.
/// Replace every hand-written confirm-dialog hint array with `CONFIRM_KEYMAP.hint_spans()`.
pub static CONFIRM_KEYMAP: Keymap<ConfirmAction> = Keymap::new(CONFIRM_BINDINGS);

/// Return hint spans for the confirm dialog from the authoritative registry.
/// Prefer calling this rather than hand-writing a hint array.
#[must_use]
pub fn confirm_hint_spans() -> Vec<HintSpan<'static>> {
    CONFIRM_KEYMAP.hint_spans()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmFocus {
    Yes,
    No,
}

impl ButtonFocus for ConfirmFocus {
    const RING: &'static [Self] = &[Self::Yes, Self::No];
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub(crate) focus: ConfirmFocus,
    pub(crate) title: String,
    pub(crate) kind: ConfirmKind,
}

/// Discriminated payload for the Confirm modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    Default {
        prompt: String,
    },
    Details {
        prompt: String,
        rows: Vec<(String, String)>,
        notes: Vec<String>,
    },
}

impl ConfirmState {
    /// Build a new Confirm modal. Default focus = No, so Enter does not
    /// accidentally commit Yes for destructive actions.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            focus: ConfirmFocus::No,
            title: "Confirm".into(),
            kind: ConfirmKind::Default {
                prompt: prompt.into(),
            },
        }
    }

    #[must_use]
    pub fn details(
        title: impl Into<String>,
        prompt: impl Into<String>,
        rows: Vec<(String, String)>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            focus: ConfirmFocus::No,
            title: title.into(),
            kind: ConfirmKind::Details {
                prompt: prompt.into(),
                rows,
                notes,
            },
        }
    }

    /// Set focus to Yes. Allows callers outside this crate to pre-select
    /// Yes when the state reflects an already-confirmed choice.
    #[must_use]
    pub fn with_focus_yes(mut self) -> Self {
        self.focus = ConfirmFocus::Yes;
        self
    }

    /// Set focus to No. Allows callers outside this crate to pre-select
    /// No (the default, but useful when building state from a stored value).
    #[must_use]
    pub fn with_focus_no(mut self) -> Self {
        self.focus = ConfirmFocus::No;
        self
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn kind(&self) -> &ConfirmKind {
        &self.kind
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool> {
        match CONFIRM_KEYMAP.dispatch(KeyChord::from(key)) {
            Some(ConfirmAction::Yes) => ModalOutcome::Commit(true),
            Some(ConfirmAction::No) => ModalOutcome::Commit(false),
            Some(ConfirmAction::Cancel) => ModalOutcome::Cancel,
            Some(ConfirmAction::ToggleFocus) => {
                self.focus = self.focus.next();
                ModalOutcome::Continue
            }
            Some(ConfirmAction::CommitFocused) => {
                ModalOutcome::Commit(matches!(self.focus, ConfirmFocus::Yes))
            }
            None => ModalOutcome::Continue,
        }
    }
}

/// Height this Confirm modal wants, given its current contents.
#[must_use]
pub fn required_height(state: &ConfirmState) -> u16 {
    match &state.kind {
        ConfirmKind::Details { rows, notes, .. } => {
            // 2 borders + 1 leading + prompt + sep + detail_rows + sep + note_rows + spacer + buttons + 1 trailing
            let content_rows = 1usize // leading spacer
                .saturating_add(1) // prompt
                .saturating_add(1) // separator
                .saturating_add(rows.len()) // detail rows
                .saturating_add(1) // separator
                .saturating_add(notes.len()) // note rows
                .saturating_add(1) // spacer before buttons
                .saturating_add(1) // buttons
                .saturating_add(1); // trailing spacer
            u16::try_from(content_rows.saturating_add(2)).unwrap_or(u16::MAX)
        }
        ConfirmKind::Default { prompt } => {
            // 2 borders + 1 leading + prompt_lines + 1 spacer + 1 buttons + 1 trailing
            let prompt_lines = prompt.lines().count().max(1) as u16;
            prompt_lines + 6
        }
    }
}

#[must_use]
pub const fn width_pct(state: &ConfirmState) -> u16 {
    match &state.kind {
        ConfirmKind::Default { .. } => 60,
        ConfirmKind::Details { .. } => 70,
    }
}

/// The canonical "Exit jackin❯?" confirmation, shared by every host surface
/// that can quit the app (console, launch cockpit). One construction site keeps
/// the wording and shape identical everywhere. Default focus = Yes because the
/// operator already invoked the explicit quit chord; destructive data-loss
/// confirmations still use [`ConfirmState::new`] / [`ConfirmState::details`]
/// and default to No.
#[must_use]
pub fn exit_confirm_state() -> ConfirmState {
    ConfirmState::new("Exit jackin❯?").with_focus_yes()
}

/// Exit confirmation for surfaces where quitting force-stops the container and
/// destroys in-container state (the capsule). Same prompt as
/// [`exit_confirm_state`], plus warning notes the operator must accept. Default
/// focus = No.
#[must_use]
pub fn exit_confirm_state_with_data_loss() -> ConfirmState {
    ConfirmState::details(
        "Confirm",
        "Exit jackin❯?",
        Vec::new(),
        vec![
            "Exiting force-stops the container immediately.".into(),
            "Work not saved outside the container will be lost.".into(),
        ],
    )
}

pub fn render_confirm_dialog(frame: &mut Frame<'_>, area: Rect, state: &ConfirmState) {
    let inner = render_dialog_shell(frame, area, Some(&state.title), DialogBorder::Default);

    let prompt = match &state.kind {
        ConfirmKind::Details {
            prompt,
            rows,
            notes,
        } => {
            render_details(frame, inner, state, prompt, rows, notes);
            return;
        }
        ConfirmKind::Default { prompt } => prompt.as_str(),
    };

    let prompt_lines = prompt.lines().count().max(1) as u16;
    let chunks = dialog_inner_chunks(inner, Some(prompt_lines));

    let prompt_lines_vec: Vec<Line<'_>> = prompt
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            let style = if idx == 0 {
                crate::theme::BOLD_WHITE
            } else {
                crate::theme::DIM
            };
            Line::from(Span::styled(line.to_owned(), style))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(prompt_lines_vec).alignment(Alignment::Center),
        chunks[1],
    );

    render_buttons(frame, chunks[3], state);
}

#[must_use]
pub fn confirm_button_hit(
    dialog_area: Rect,
    state: &ConfirmState,
    col: u16,
    row: u16,
) -> Option<bool> {
    let button_area = confirm_button_area(dialog_area, state)?;
    let items = [ButtonStripItem::new("Yes"), ButtonStripItem::new("No")];
    ButtonStrip::new(&items)
        .focused(state.focus.index())
        .button_rects(button_area)
        .into_iter()
        .enumerate()
        .find(|(_, rect)| {
            row >= rect.y
                && row < rect.y.saturating_add(rect.height)
                && col >= rect.x
                && col < rect.x.saturating_add(rect.width)
        })
        .map(|(idx, _)| idx == 0)
}

fn confirm_button_area(dialog_area: Rect, state: &ConfirmState) -> Option<Rect> {
    if dialog_area.width < 2 || dialog_area.height < 2 {
        return None;
    }
    let inner = Rect {
        x: dialog_area.x.saturating_add(1),
        y: dialog_area.y.saturating_add(1),
        width: dialog_area.width.saturating_sub(2),
        height: dialog_area.height.saturating_sub(2),
    };
    match &state.kind {
        ConfirmKind::Default { prompt } => {
            let prompt_lines = prompt.lines().count().max(1) as u16;
            let chunks = dialog_inner_chunks(inner, Some(prompt_lines));
            Some(chunks[3])
        }
        ConfirmKind::Details { rows, notes, .. } => {
            let detail_rows = u16::try_from(rows.len()).unwrap_or(u16::MAX);
            let note_rows = u16::try_from(notes.len()).unwrap_or(u16::MAX);
            let rows = confirm_details_chunks(inner, detail_rows, note_rows);
            Some(rows[7])
        }
    }
}

fn render_details(
    frame: &mut Frame<'_>,
    inner: Rect,
    state: &ConfirmState,
    prompt: &str,
    details: &[(String, String)],
    notes: &[String],
) {
    let detail_rows = u16::try_from(details.len()).unwrap_or(u16::MAX);
    let note_rows = u16::try_from(notes.len()).unwrap_or(u16::MAX);
    // Canonical dialog layout: leading spacer + content + spacer + buttons + trailing spacer.
    let rows = confirm_details_chunks(inner, detail_rows, note_rows);

    let key = crate::theme::BOLD_WHITE;
    let value = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let note = crate::theme::DIM;

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            prompt.to_owned(),
            crate::theme::BOLD_WHITE,
        ))),
        inset(rows[1], 3),
    );

    let detail_lines = details
        .iter()
        .map(|(label, value_text)| {
            Line::from(vec![
                Span::styled(format!("{label}: "), key),
                Span::styled(value_text.clone(), value),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(detail_lines), inset(rows[3], 3));

    let note_lines = notes
        .iter()
        .map(|message| {
            Line::from(vec![
                Span::styled(
                    "!",
                    Style::default()
                        .fg(WARNING_YELLOW)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(message.clone(), note),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(note_lines), inset(rows[5], 3));

    render_buttons(frame, rows[7], state);
}

fn confirm_details_chunks(inner: Rect, detail_rows: u16, note_rows: u16) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // rows[0]: leading spacer
            Constraint::Length(1),           // rows[1]: prompt
            Constraint::Length(1),           // rows[2]: separator
            Constraint::Length(detail_rows), // rows[3]: detail rows
            Constraint::Length(1),           // rows[4]: separator
            Constraint::Length(note_rows),   // rows[5]: note rows
            Constraint::Length(1),           // rows[6]: spacer before buttons
            Constraint::Length(1),           // rows[7]: buttons
            Constraint::Length(1),           // rows[8]: trailing spacer
        ])
        .split(inner)
}

const fn inset(area: Rect, x: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(x),
        y: area.y,
        width: area.width.saturating_sub(x.saturating_mul(2)),
        height: area.height,
    }
}

fn render_buttons(frame: &mut Frame<'_>, area: Rect, state: &ConfirmState) {
    let items = [ButtonStripItem::new("Yes"), ButtonStripItem::new("No")];
    frame.render_widget(ButtonStrip::new(&items).focused(state.focus.index()), area);
}

#[cfg(test)]
mod tests;
