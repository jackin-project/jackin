// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rendering for the shared 1Password picker modal.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::components::spinner::SPINNER_FRAMES;
use termrock::widgets::{List, ListRow, ListState, RowRole, TextInput, TextInputState, Validation};

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

fn render_pane(frame: &mut Frame<'_>, area: Rect, state: &impl OpPickerRenderState) {
    let multi_account = state.account_count() > 1;

    if let Some(input) = state.naming_stage_input() {
        let donor = crate::tui::components::TextInputState::new(input.label(), input.value());
        crate::tui::components::render_text_input(frame, area, &donor);
        return;
    }

    let title = breadcrumb_title(
        state.stage(),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    );
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some(&title),
        termrock::widgets::PanelEmphasis::Focused,
        &termrock::Theme::default(),
    );

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

    if banner_height > 0
        && let OpLoadState::Error(OpPickerError::Recoverable { message }) = state.load_state()
    {
        let truncated: String = message.chars().take(120).collect();
        let line = Line::from(vec![
            Span::styled("Error: ", jackin_ui::theme::text_strong()),
            Span::styled(truncated, jackin_ui::theme::text_muted()),
        ]);
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    let theme = termrock::Theme::default();
    let mut filter = TextInputState::new(state.filter_buffer()).with_allow_empty(true);
    frame.render_stateful_widget(
        &TextInput::new("Filter", &theme)
            .placeholder("Filter")
            .validation(Validation::Valid),
        rows[1],
        &mut filter,
    );

    let list_lines = match state.stage() {
        OpPickerStage::Account => state.account_lines(),
        OpPickerStage::Vault => state.vault_lines(),
        OpPickerStage::Item => state.item_lines(),
        OpPickerStage::Section => state.section_lines(),
        OpPickerStage::Field => state.field_lines(),
        OpPickerStage::NewItemName | OpPickerStage::FieldLabel | OpPickerStage::NewSectionName => {
            Vec::new()
        }
    };
    if list_lines.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "(no matches)",
            jackin_ui::theme::text_muted(),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(para, rows[3]);
    } else {
        let items = list_lines
            .into_iter()
            .enumerate()
            .map(|(id, label)| ListRow {
                id,
                label,
                trailing: None,
                role: RowRole::Item,
                enabled: true,
            })
            .collect::<Vec<_>>();
        frame.render_stateful_widget(
            &List::new(&items, &theme),
            rows[3],
            &mut ListState::new(state.selected_index()),
        );
    }
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
        termrock::widgets::PanelEmphasis::Focused,
        &termrock::Theme::default(),
    );

    let glyph = SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()];
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
        Span::styled(glyph.to_owned(), jackin_ui::theme::accent()),
        Span::raw("  "),
        Span::styled(descriptor, jackin_ui::theme::text_muted()),
    ]);
    frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[1]);
}

pub fn render_fatal(frame: &mut Frame<'_>, area: Rect, fatal: &OpPickerFatalState) {
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some("1Password"),
        termrock::widgets::PanelEmphasis::Focused,
        &termrock::Theme::default(),
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
