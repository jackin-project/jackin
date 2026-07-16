// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch prompt dialog rendering and geometry.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;
use termrock::interaction::Outcome;
use termrock::widgets::{
    Action, ChoiceDialog, ChoiceDialogState, Dialog, List, ListRow, ListState, MessageDialog,
    PanelEmphasis, RowRole, TextInput, TextInputOutcome, TextInputState, Validation,
};
use termrock::{Theme, widgets::HintSpan};

use crate::tui::components::dialog::dialog_backdrop;
use crate::tui::components::dialog::{exact_dialog_rect, percent_dialog_rect};

#[derive(Debug)]
pub struct PromptPicker {
    items: Vec<String>,
    filtered: Vec<usize>,
    filter: String,
    state: ListState<usize>,
}

impl PromptPicker {
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        let filtered = (0..items.len()).collect::<Vec<_>>();
        Self {
            state: ListState::new(filtered.first().copied()),
            items,
            filtered,
            filter: String::new(),
        }
    }

    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self.recompute();
        self
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn max_label_width(&self) -> u16 {
        self.items
            .iter()
            .map(|label| termrock::text::display_cols(label))
            .max()
            .unwrap_or(0)
            .try_into()
            .unwrap_or(u16::MAX)
    }

    #[must_use]
    pub const fn selected_index(&self) -> Option<usize> {
        self.state.selected().copied()
    }

    pub fn select_index(&mut self, index: usize) {
        if self.filtered.contains(&index) {
            self.state.select(Some(index));
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome<usize> {
        match key.code {
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.recompute();
                    Outcome::Changed
                } else {
                    Outcome::Ignored
                }
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.filter.push(character);
                self.recompute();
                Outcome::Changed
            }
            _ => {
                let rows = self.rows();
                self.state.handle_key(&rows, key.into())
            }
        }
    }

    fn recompute(&mut self) {
        let needle = self.filter.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, label)| {
                (needle.is_empty() || label.to_lowercase().contains(&needle)).then_some(index)
            })
            .collect();
        self.state.select(self.filtered.first().copied());
        self.state.scroll_by(isize::MIN, self.filtered.len());
    }

    fn rows(&self) -> Vec<ListRow<'static, usize>> {
        self.filtered
            .iter()
            .map(|index| ListRow {
                id: *index,
                label: Line::from(self.items[*index].clone()),
                trailing: None,
                role: RowRole::Item,
                enabled: true,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PromptText {
    pub label: String,
    pub state: TextInputState,
}

impl PromptText {
    #[must_use]
    pub fn new(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: TextInputState::new(initial),
        }
    }

    #[must_use]
    pub fn new_allow_empty(label: impl Into<String>, initial: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: TextInputState::new(initial).with_allow_empty(true),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> TextInputOutcome {
        self.state.handle_key(key.into())
    }
}

#[derive(Debug, Clone)]
pub struct PromptConfirm {
    pub title: String,
    pub prompt: String,
    pub rows: Vec<(String, String)>,
    pub notes: Vec<String>,
    pub state: ChoiceDialogState<bool>,
}

impl PromptConfirm {
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            title: "Confirm".to_owned(),
            prompt: prompt.into(),
            rows: Vec::new(),
            notes: Vec::new(),
            state: ChoiceDialogState::new(Some(false)),
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
            title: title.into(),
            prompt: prompt.into(),
            rows,
            notes,
            state: ChoiceDialogState::new(Some(false)),
        }
    }

    #[must_use]
    pub fn with_focus_yes(mut self) -> Self {
        self.state.focused = Some(true);
        self
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome<bool> {
        match key.code {
            KeyCode::Char('y' | 'Y') => Outcome::Activated(true),
            KeyCode::Char('n' | 'N') => Outcome::Activated(false),
            _ => self.state.handle_key(&confirm_actions(), key.into()),
        }
    }

    #[must_use]
    pub fn required_height(&self) -> u16 {
        let prompt = self.prompt.lines().count().max(1);
        let rows = self.rows.len();
        let notes = self.notes.len();
        u16::try_from(
            prompt
                .saturating_add(rows)
                .saturating_add(notes)
                .saturating_add(6),
        )
        .unwrap_or(u16::MAX)
    }

    #[must_use]
    pub const fn width_percent(&self) -> u16 {
        if self.rows.is_empty() && self.notes.is_empty() {
            60
        } else {
            70
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptError {
    pub title: String,
    pub message: String,
    state: termrock::widgets::DetailTableState<usize>,
}

impl PromptError {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            state: termrock::widgets::DetailTableState::default(),
        }
    }

    #[must_use]
    pub fn required_height(&self, width: u16, max_height: u16) -> u16 {
        let content_width = usize::from(width.saturating_sub(2)).max(1);
        let rows = self
            .message
            .lines()
            .map(|line| {
                termrock::text::display_cols(line)
                    .div_ceil(content_width)
                    .max(1)
            })
            .sum::<usize>();
        u16::try_from(rows.saturating_add(2))
            .unwrap_or(u16::MAX)
            .clamp(3, max_height.max(3))
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => Outcome::Cancelled,
            _ => Outcome::Ignored,
        }
    }
}

fn confirm_actions() -> [Action<'static, bool>; 2] {
    [
        Action {
            id: true,
            label: "Yes",
            enabled: true,
            style: None,
        },
        Action {
            id: false,
            label: "No",
            enabled: true,
            style: None,
        },
    ]
}

fn select_list_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
        HintSpan::GroupSep,
        HintSpan::Text("type to filter"),
    ]
}

pub(crate) fn confirm_hint_spans() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("confirm"),
        HintSpan::GroupSep,
        HintSpan::Key("Y"),
        HintSpan::Text("yes"),
        HintSpan::GroupSep,
        HintSpan::Key("N/Esc"),
        HintSpan::Text("no"),
        HintSpan::GroupSep,
        HintSpan::Key("⇥"),
        HintSpan::Text("focus"),
    ]
}

fn error_popup_hint_spans() -> Vec<HintSpan<'static>> {
    vec![HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")]
}

pub fn draw_select(
    frame: &mut Frame<'_>,
    title: &str,
    context: &[Line<'_>],
    picker: &mut PromptPicker,
) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    let area = picker_rect(box_area, picker, context);
    render_picker(frame, area, title, context, picker);
    termrock::widgets::render_hint_bar(
        frame,
        hint_area,
        &select_list_hint_spans(),
        &Theme::default(),
    );
}

pub(crate) fn render_picker(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    context: &[Line<'_>],
    picker: &mut PromptPicker,
) {
    let theme = Theme::default();
    let panel = termrock::widgets::Panel::new(&theme)
        .title(title)
        .emphasis(PanelEmphasis::Focused);
    let inner = panel.inner(area);
    frame.render_widget(&panel, area);
    let context_height = u16::try_from(context.len()).unwrap_or(u16::MAX);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(context_height),
            Constraint::Min(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(format!("Filter: {}", picker.filter)),
        rows[0],
    );
    if !context.is_empty() {
        frame.render_widget(Paragraph::new(context.to_vec()), rows[1]);
    }
    let list_rows = picker.rows();
    frame.render_stateful_widget(&List::new(&list_rows, &theme), rows[2], &mut picker.state);
}

pub fn draw_text_prompt(frame: &mut Frame<'_>, input: &mut PromptText, skippable: bool) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    let area = text_input_prompt_rect(box_area);
    let theme = Theme::default();
    let panel = termrock::widgets::Panel::new(&theme)
        .title(&input.label)
        .emphasis(PanelEmphasis::Focused);
    let inner = panel.inner(area);
    frame.render_widget(&panel, area);
    frame.render_stateful_widget(
        &TextInput::new(&input.label, &theme)
            .placeholder("")
            .validation(Validation::Valid),
        inner.inner(ratatui::layout::Margin::new(1, 1)),
        &mut input.state,
    );
    termrock::widgets::render_hint_bar(
        frame,
        hint_area,
        text_prompt_hint(skippable),
        &Theme::default(),
    );
}

pub fn draw_confirm(frame: &mut Frame<'_>, state: &mut PromptConfirm) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    let area = confirm_rect(box_area, state);
    let mut body = vec![Line::from(state.prompt.clone())];
    body.extend(
        state
            .rows
            .iter()
            .map(|(label, value)| Line::from(format!("{label}: {value}"))),
    );
    body.extend(
        state
            .notes
            .iter()
            .map(|note| Line::from(format!("! {note}"))),
    );
    let theme = Theme::default();
    let actions = confirm_actions();
    let dialog = Dialog::new(&state.title, Text::from(body), &theme)
        .style(Style::default())
        .emphasis(PanelEmphasis::Focused);
    frame.render_stateful_widget(
        &ChoiceDialog::new(dialog, &actions).gap(" "),
        area,
        &mut state.state,
    );
    termrock::widgets::render_hint_bar(frame, hint_area, &confirm_hint_spans(), &Theme::default());
}

pub fn draw_error_popup(frame: &mut Frame<'_>, state: &mut PromptError) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    let area = error_popup_rect(box_area, state);
    let theme = Theme::default();
    let dialog = Dialog::new(&state.title, Text::from(state.message.as_str()), &theme)
        .style(Style::default())
        .emphasis(PanelEmphasis::Focused);
    frame.render_stateful_widget(
        &MessageDialog::new(dialog, &[], &theme).wrap(true),
        area,
        &mut state.state,
    );
    termrock::widgets::render_hint_bar(
        frame,
        hint_area,
        &error_popup_hint_spans(),
        &Theme::default(),
    );
}

fn picker_rect(area: Rect, picker: &PromptPicker, context: &[Line<'_>]) -> Rect {
    // Structural exception: launch picker size depends on transient context lines before the shared select-list renderer runs.
    // Interior: filter row + spacer + one row per item, plus two borders; a
    // non-empty context block adds its line count plus a spacer.
    let context_rows = u16::try_from(context.len()).unwrap_or(u16::MAX);
    let context_extra = if context_rows > 0 {
        context_rows.saturating_add(1)
    } else {
        0
    };
    let rows = u16::try_from(picker.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .saturating_add(context_extra);
    let height = rows.clamp(6, area.height.saturating_sub(2).max(6));
    let min_w = 40.min(area.width);
    let max_w = (area.width.saturating_mul(4) / 5).max(min_w);
    let context_w = context
        .iter()
        .map(|line| u16::try_from(line.width()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0);
    let width = picker
        .max_label_width()
        .max(context_w)
        .saturating_add(6)
        .clamp(min_w, max_w);
    exact_dialog_rect(area, width, height)
}

fn confirm_rect(area: Rect, state: &PromptConfirm) -> Rect {
    percent_dialog_rect(
        area,
        state.width_percent(),
        0,
        2,
        2,
        state.required_height(),
    )
}

fn error_popup_rect(area: Rect, state: &PromptError) -> Rect {
    let width = (area.width.saturating_mul(3) / 4).clamp(40, area.width.max(40));
    let height = state.required_height(width.saturating_sub(2), area.height);
    exact_dialog_rect(area, width, height)
}

fn text_input_prompt_rect(area: Rect) -> Rect {
    let min_width = 50.min(area.width);
    let width = (area.width.saturating_mul(3) / 5).clamp(min_width, area.width.max(min_width));
    exact_dialog_rect(area, width, 5)
}

/// Footer-hint keys for the launch text prompt. `skippable` adds the
/// leave-empty-to-skip group; both share the rest of the vocabulary.
const fn text_prompt_hint(skippable: bool) -> &'static [HintSpan<'static>] {
    if skippable {
        TEXT_PROMPT_SKIP_HINT
    } else {
        TEXT_PROMPT_HINT
    }
}

const TEXT_PROMPT_HINT: &[HintSpan<'static>] = &[
    // UNREGISTERABLE(text-prompt-no-keymap): Enter confirms the field inline; no TEXT_PROMPT_KEYMAP registered.
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    // UNREGISTERABLE(multi-key-display-group): combined Ctrl-C/Ctrl-Q/Esc cancel display.
    HintSpan::Key("Ctrl-C/Ctrl-Q/Esc"),
    HintSpan::Text("cancel"),
];

const TEXT_PROMPT_SKIP_HINT: &[HintSpan<'static>] = &[
    // UNREGISTERABLE(text-prompt-no-keymap): Enter confirms the field inline; no TEXT_PROMPT_KEYMAP registered.
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    // UNREGISTERABLE(dynamic-input-instruction): "empty" is a display label for the skip affordance, not a key.
    HintSpan::Key("empty"),
    HintSpan::Text("skip"),
    HintSpan::GroupSep,
    // UNREGISTERABLE(multi-key-display-group): combined Ctrl-C/Ctrl-Q/Esc cancel display.
    HintSpan::Key("Ctrl-C/Ctrl-Q/Esc"),
    HintSpan::Text("cancel"),
];

#[cfg(test)]
mod tests;
