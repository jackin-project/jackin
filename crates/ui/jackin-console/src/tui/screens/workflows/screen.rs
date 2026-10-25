// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use super::effect::WorkflowsEffect;
use super::message::WorkflowsMessage;
use super::model::{WorkflowAction, WorkflowScreen};
use super::update::apply_workflows_message;
use super::view::{confirm_prompt, render_workflow_detail, render_workflow_list};
use crate::tui::effect::ConsoleEffect;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{ManagerEffect, ManagerStage, ManagerState};
use crate::tui::view::{render_footer, render_header, workspace_frame_areas};

fn screen_mut<'a>(state: &'a mut ManagerState<'_>) -> Option<&'a mut WorkflowScreen> {
    match &mut state.stage {
        ManagerStage::Workflows(screen) => Some(screen),
        _ => None,
    }
}

fn request_effects(state: &mut ManagerState<'_>, effects: Vec<WorkflowsEffect>) {
    for effect in effects {
        state.request_effect(ManagerEffect::Workflows(effect));
    }
}

fn request_refresh(state: &mut ManagerState<'_>) {
    state.request_effect(ConsoleEffect::RequestWorkflowsRefresh.into());
}

pub fn handle_key(state: &mut ManagerState<'_>, key: KeyEvent, export_dir: &Path) {
    let Some(screen) = screen_mut(state) else {
        return;
    };
    if screen.confirm().is_some() {
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                let effects = apply_workflows_message(screen, WorkflowsMessage::ConfirmAction);
                request_effects(state, effects);
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                apply_workflows_message(screen, WorkflowsMessage::DismissConfirm);
            }
            _ => {}
        }
        return;
    }
    let message = match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            if screen.detail().is_some() {
                WorkflowsMessage::CloseDetail
            } else {
                update_manager(state, ManagerMessage::ReturnToList);
                return;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => WorkflowsMessage::SelectPrevious,
        KeyCode::Down | KeyCode::Char('j') => WorkflowsMessage::SelectNext,
        KeyCode::Enter => WorkflowsMessage::OpenDetail,
        KeyCode::Char('o') => WorkflowsMessage::Observe("implementer".to_owned()),
        KeyCode::Char('t') => WorkflowsMessage::RequestAction(WorkflowAction::TakeControl),
        KeyCode::Char('r') => WorkflowsMessage::RequestAction(WorkflowAction::Resume),
        KeyCode::Char('c') => WorkflowsMessage::RequestAction(WorkflowAction::Cancel),
        KeyCode::Char('s') => WorkflowsMessage::StopObserving {
            return_control: true,
        },
        KeyCode::Char('e') => {
            WorkflowsMessage::RequestExport(export_dir.to_string_lossy().into_owned())
        }
        _ => return,
    };
    let needs_refresh = matches!(
        message,
        WorkflowsMessage::SelectNext
            | WorkflowsMessage::SelectPrevious
            | WorkflowsMessage::OpenDetail
    );
    let Some(screen) = screen_mut(state) else {
        return;
    };
    let effects = apply_workflows_message(screen, message);
    request_effects(state, effects);
    if needs_refresh {
        request_refresh(state);
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let areas = workspace_frame_areas(area);
    render_header(frame, areas.header, "workflows");
    let ManagerStage::Workflows(screen) = &state.stage else {
        return;
    };
    let mut body = areas.body;
    if let Some(error) = screen.error() {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(body);
        frame.render_widget(
            Paragraph::new(Line::from(format!("error: {error}"))),
            rows[0],
        );
        body = rows[1];
    }
    if screen.detail().is_some() {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(body);
        render_workflow_list(frame, columns[0], screen);
        render_workflow_detail(frame, columns[1], screen);
    } else {
        render_workflow_list(frame, body, screen);
    }
    if let Some(action) = screen.confirm()
        && let Some(detail) = screen.detail()
    {
        let prompt = confirm_prompt(action, &detail.summary.run_id);
        let dialog = crate::tui::layout::centered_rect_fixed(area, 70, 7);
        frame.render_widget(
            Paragraph::new(vec![Line::from(prompt), Line::from("y confirm / n cancel")])
                .block(Block::default().borders(Borders::ALL).title(action.label())),
            dialog,
        );
    }
    render_footer(frame, areas.footer, &workflow_footer_hints());
}

fn workflow_footer_hints() -> [termrock::widgets::HintSpan<'static>; 18] {
    use termrock::widgets::HintSpan::{Key, Text};
    [
        Key("↑↓"),
        Text(" select  "),
        Key("↵"),
        Text(" open  "),
        Key("o"),
        Text(" observe  "),
        Key("t"),
        Text(" take  "),
        Key("r"),
        Text(" resume  "),
        Key("c"),
        Text(" cancel  "),
        Key("e"),
        Text(" export  "),
        Key("s"),
        Text(" release  "),
        Key("Esc"),
        Text(" back"),
    ]
}
