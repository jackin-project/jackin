// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch prompt dialog rendering and geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use termrock::HintSpan;
use termrock::components::{
    ConfirmState, ErrorPopupState, SelectListState, TextInputState, confirm_required_height,
    confirm_width_pct, render_confirm_dialog, render_error_dialog_in, render_select_list,
    render_text_input, required_height as error_dialog_required_height, text_input_prompt_rect,
};

use crate::tui::components::dialog::dialog_backdrop;
use crate::tui::components::dialog::{exact_dialog_rect, percent_dialog_rect};

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
    picker: &SelectListState,
) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_select_list(
        frame,
        picker_rect(box_area, picker, context),
        picker,
        title,
        context,
    );
    termrock::widgets::render_hint_bar(frame, hint_area, &select_list_hint_spans());
}

pub fn draw_text_prompt(frame: &mut Frame<'_>, input: &TextInputState<'_>, skippable: bool) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_text_input(frame, text_input_prompt_rect(box_area), input);
    termrock::widgets::render_hint_bar(frame, hint_area, text_prompt_hint(skippable));
}

pub fn draw_confirm(frame: &mut Frame<'_>, state: &ConfirmState) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_confirm_dialog(frame, confirm_rect(box_area, state), state);
    termrock::widgets::render_hint_bar(frame, hint_area, &confirm_hint_spans());
}

pub fn draw_error_popup(frame: &mut Frame<'_>, state: &ErrorPopupState) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_error_dialog_in(frame, error_popup_rect(box_area, state), state);
    termrock::widgets::render_hint_bar(frame, hint_area, &error_popup_hint_spans());
}

fn picker_rect(area: Rect, picker: &SelectListState, context: &[Line<'_>]) -> Rect {
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

fn confirm_rect(area: Rect, state: &ConfirmState) -> Rect {
    percent_dialog_rect(
        area,
        confirm_width_pct(state),
        0,
        2,
        2,
        confirm_required_height(state),
    )
}

fn error_popup_rect(area: Rect, state: &ErrorPopupState) -> Rect {
    let width = (area.width.saturating_mul(3) / 4).clamp(40, area.width.max(40));
    let height = error_dialog_required_height(state, width.saturating_sub(2), area.height);
    exact_dialog_rect(area, width, height)
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
