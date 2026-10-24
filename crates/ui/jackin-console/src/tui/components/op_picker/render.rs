// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    text::{Line, Span},
    widgets::{Paragraph, StatefulWidget},
};

use crate::tui::components::spinner::console_spinner_frame;
use termrock::widgets::{
    List, ListRow, ListState, Panel, PanelChrome, TextInput, TextInputState, Validation,
};

use super::{
    OpLoadState, OpPickerError, OpPickerFatalState, OpPickerRenderState, OpPickerStage,
    breadcrumb_title, fatal_body_lines, loading_descriptor, loading_title_stage,
};

pub fn render_picker(frame: &mut Frame<'_>, area: Rect, state: &impl OpPickerRenderState) {
    match state.load_state() {
        OpLoadState::Error(OpPickerError::Fatal(fatal)) => render_fatal(frame, area, fatal),
        OpLoadState::Loading { spinner_tick } => render_loading(frame, area, state, *spinner_tick),
        OpLoadState::Idle
        | OpLoadState::Ready
        | OpLoadState::Error(OpPickerError::Recoverable { .. }) => {
            render_pane(frame, area, state);
        }
    }
}

pub(in crate::tui) fn selection_hit_at(
    area: Rect,
    state: &impl OpPickerRenderState,
    position: Position,
) -> bool {
    if area.width == 0
        || area.height == 0
        || state.stage().is_naming()
        || matches!(
            state.load_state(),
            OpLoadState::Loading { .. } | OpLoadState::Error(OpPickerError::Fatal(_))
        )
    {
        return false;
    }

    let list_lines = picker_lines(state);
    if list_lines.is_empty() {
        return false;
    }

    let theme = termrock::style::DesignSystem::default();
    let title = picker_title(state);
    let inner = Panel::new(&theme)
        .emphasis(PanelChrome::Focused)
        .title(title.as_str())
        .block()
        .inner(area);
    let list_area = pane_rows(inner, state)[3];
    if list_area.width == 0 || list_area.height == 0 {
        return false;
    }

    let items = picker_rows(list_lines);
    let mut list_state = ListState::new(state.selected_index());
    let mut buffer = Buffer::empty(list_area);
    (&List::new(&items, &theme)).render(list_area, &mut buffer, &mut list_state);

    list_state
        .regions()
        .iter()
        .any(|region| region.area.contains(position))
}

fn render_pane(frame: &mut Frame<'_>, area: Rect, state: &impl OpPickerRenderState) {
    if let Some(input) = state.naming_stage_input() {
        let donor = crate::tui::components::TextInputState::new(input.label(), input.value());
        crate::tui::components::render_text_input(frame, area, &donor);
        return;
    }

    let title = picker_title(state);
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some(&title),
        PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );
    let rows = pane_rows(inner, state);

    if let OpLoadState::Error(OpPickerError::Recoverable { message }) = state.load_state() {
        let truncated: String = message.chars().take(120).collect();
        let line = Line::from(vec![
            Span::styled(
                "Error: ",
                termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
            ),
            Span::styled(
                truncated,
                termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    let theme = termrock::style::DesignSystem::default();
    let mut filter = TextInputState::new(state.filter_buffer()).with_allow_empty(true);
    frame.render_stateful_widget(
        &TextInput::new("Filter", &theme)
            .placeholder("Filter")
            .validation(Validation::Valid),
        rows[1],
        &mut filter,
    );

    let list_lines = picker_lines(state);
    if list_lines.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "(no matches)",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(para, rows[3]);
    } else {
        let items = picker_rows(list_lines);
        frame.render_stateful_widget(
            &List::new(&items, &theme),
            rows[3],
            &mut ListState::new(state.selected_index()),
        );
    }
}

fn picker_title(state: &impl OpPickerRenderState) -> String {
    breadcrumb_title(
        state.stage(),
        state.account_count() > 1,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    )
}

fn pane_rows(inner: Rect, state: &impl OpPickerRenderState) -> [Rect; 4] {
    let banner_height: u16 = match state.load_state() {
        OpLoadState::Error(OpPickerError::Recoverable { .. }) => 2,
        _ => 0,
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(banner_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    [rows[0], rows[1], rows[2], rows[3]]
}

fn picker_lines(state: &impl OpPickerRenderState) -> Vec<Line<'static>> {
    match state.stage() {
        OpPickerStage::Account => state.account_lines(),
        OpPickerStage::Vault => state.vault_lines(),
        OpPickerStage::Item => state.item_lines(),
        OpPickerStage::Section => state.section_lines(),
        OpPickerStage::Field => state.field_lines(),
        OpPickerStage::NewItemName | OpPickerStage::FieldLabel | OpPickerStage::NewSectionName => {
            Vec::new()
        }
    }
}

fn picker_rows(list_lines: Vec<Line<'static>>) -> Vec<ListRow<'static, usize>> {
    list_lines
        .into_iter()
        .enumerate()
        .map(|(id, label)| ListRow::item(id, label))
        .collect()
}

fn render_loading(frame: &mut Frame<'_>, area: Rect, state: &impl OpPickerRenderState, tick: u8) {
    let multi_account = state.account_count() > 1;
    let title = breadcrumb_title(
        loading_title_stage(state.stage()),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    );
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some(&title),
        PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );

    let glyph = console_spinner_frame(tick);
    let descriptor = loading_descriptor(
        state.stage(),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
        state.selected_item_subtitle(),
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let body = Line::from(vec![
        Span::styled(
            glyph.to_owned(),
            termrock::style::DesignSystem::default().style(termrock::style::Role::Accent),
        ),
        Span::raw("  "),
        Span::styled(
            descriptor,
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted),
        ),
    ]);
    frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[1]);
}

pub fn render_fatal(frame: &mut Frame<'_>, area: Rect, fatal: &OpPickerFatalState) {
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some("1Password"),
        PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(fatal_body_lines(fatal)).alignment(Alignment::Center),
        rows[1],
    );
}
