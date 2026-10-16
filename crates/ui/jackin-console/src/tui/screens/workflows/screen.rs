// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workflows stage routing: key handling, surface rendering, and
//! footer hints. Navigation maps keys to [`WorkflowsMessage`] through
//! the pure screen update; every returned [`WorkflowsEffect`] is
//! requested from the host, which owns the daemon and Capsule
//! clients. List and detail snapshots arrive through
//! `RequestWorkflowsRefresh`, executed by the same host.

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

/// Handle one key while the Workflows stage is focused. `export_dir`
/// is the host-owned directory exports are written to; it comes from
/// the host paths, never from screen state.
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

/// Render the workflows surface: header, run list plus run detail,
/// an inline confirmation dialog for destructive actions, and the
/// stage footer.
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

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;
    use jackin_workflow::{RunDetail, RunSummary, StoredSession};

    use super::super::model::{ObserveTarget, WorkflowAction, WorkflowScreen};
    use super::handle_key;
    use crate::tui::effect::ConsoleEffect;
    use crate::tui::model::ConsoleManagerStageRoute;
    use crate::tui::screens::workflows::effect::WorkflowsEffect;
    use crate::tui::state::update::{ManagerMessage, update_manager};
    use crate::tui::state::{ManagerEffect, ManagerStage, ManagerState};

    fn summary(run_id: &str) -> RunSummary {
        RunSummary {
            run_id: run_id.to_owned(),
            plan_hash: "plan".to_owned(),
            status: "paused".to_owned(),
            stage: "implement".to_owned(),
            round: 0,
            candidate: None,
            candidate_generation: 0,
            version: 0,
            owner_epoch: 1,
            pause_kind: None,
            max_repair_turns: 3,
        }
    }

    fn detail_with_session() -> RunDetail {
        RunDetail {
            summary: summary("run-1"),
            sessions: vec![StoredSession {
                run_id: "run-1".to_owned(),
                worker_id: "implementer".to_owned(),
                instance_id: Some("inst-1".to_owned()),
                capsule_epoch: Some(2),
                session_id: Some(7),
                process_identity: Some("pid:9".to_owned()),
                agent_profile: None,
                auth_profile: None,
            }],
            effects: Vec::new(),
            events: Vec::new(),
            evidence: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    fn entered_state() -> ManagerState<'static> {
        let mut state = ManagerState::from_config(
            &jackin_config::AppConfig::default(),
            std::path::Path::new("/tmp"),
        );
        update_manager(
            &mut state,
            ManagerMessage::EnterWorkflows(Box::new(WorkflowScreen::new(vec![
                summary("run-1"),
                summary("run-2"),
            ]))),
        );
        let ManagerStage::Workflows(screen) = &mut state.stage else {
            panic!("enter must route to the Workflows stage");
        };
        screen.set_detail(detail_with_session());
        state
    }

    #[test]
    fn enter_routes_to_workflows_stage() {
        let state = entered_state();
        assert_eq!(state.stage.route(), ConsoleManagerStageRoute::Workflows);
    }

    #[test]
    fn navigate_requests_refresh() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/exports");
        handle_key(
            &mut state,
            crate::tui::input::fixtures::key(KeyCode::Char('j')),
            dir,
        );
        let effects = state.drain_effects();
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                ManagerEffect::Console(ConsoleEffect::RequestWorkflowsRefresh)
            )),
            "navigation must reload the selected run detail, got {effects:?}"
        );
        let ManagerStage::Workflows(screen) = &state.stage else {
            panic!("must stay on the Workflows stage");
        };
        assert_eq!(screen.selected_index(), 1);
    }

    #[test]
    fn takeover_requires_confirmation_then_emits_effect() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/exports");
        let key = crate::tui::input::fixtures::key;
        handle_key(&mut state, key(KeyCode::Char('t')), dir);
        assert!(state.drain_effects().is_empty());
        let ManagerStage::Workflows(screen) = &state.stage else {
            panic!("must stay on the Workflows stage");
        };
        assert_eq!(screen.confirm(), Some(WorkflowAction::TakeControl));
        handle_key(&mut state, key(KeyCode::Char('y')), dir);
        let effects = state.drain_effects();
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                ManagerEffect::Workflows(WorkflowsEffect::TakeControl(_))
            )),
            "confirmed takeover must reach the host, got {effects:?}"
        );
    }

    #[test]
    fn resume_cancel_share_confirmation_gating() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/exports");
        let key = crate::tui::input::fixtures::key;
        for (letter, action) in [('r', WorkflowAction::Resume), ('c', WorkflowAction::Cancel)] {
            handle_key(&mut state, key(KeyCode::Char(letter)), dir);
            assert!(state.drain_effects().is_empty());
            let ManagerStage::Workflows(screen) = &state.stage else {
                panic!("must stay on the Workflows stage");
            };
            assert_eq!(screen.confirm(), Some(action));
            handle_key(&mut state, key(KeyCode::Char('n')), dir);
            let ManagerStage::Workflows(screen) = &state.stage else {
                panic!("must stay on the Workflows stage");
            };
            assert!(screen.confirm().is_none());
            assert!(state.drain_effects().is_empty());
        }
        handle_key(&mut state, key(KeyCode::Char('r')), dir);
        handle_key(&mut state, key(KeyCode::Enter), dir);
        assert!(
            state.drain_effects().iter().any(|effect| matches!(
                effect,
                ManagerEffect::Workflows(WorkflowsEffect::Resume { .. })
            )),
            "confirmed resume must reach the host"
        );
    }

    #[test]
    fn observe_resolves_recorded_session_without_confirmation() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/exports");
        handle_key(
            &mut state,
            crate::tui::input::fixtures::key(KeyCode::Char('o')),
            dir,
        );
        let effects = state.drain_effects();
        let target = ObserveTarget {
            run_id: "run-1".to_owned(),
            worker_id: "implementer".to_owned(),
            instance_id: Some("inst-1".to_owned()),
            session_id: Some(7),
            capsule_epoch: Some(2),
            process_identity: Some("pid:9".to_owned()),
        };
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                ManagerEffect::Workflows(WorkflowsEffect::Observe(t)) if t == &target
            )),
            "observe must carry the exact recorded session, got {effects:?}"
        );
    }

    #[test]
    fn stop_observing_releases_control() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/exports");
        let key = crate::tui::input::fixtures::key;
        handle_key(&mut state, key(KeyCode::Char('o')), dir);
        state.drain_effects();
        handle_key(&mut state, key(KeyCode::Char('s')), dir);
        assert!(
            state.drain_effects().iter().any(|effect| matches!(
                effect,
                ManagerEffect::Workflows(WorkflowsEffect::ReleaseControl(_))
            )),
            "stopping must return control to the scheduler"
        );
    }

    #[test]
    fn export_carries_host_export_dir() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/workflow-exports");
        handle_key(
            &mut state,
            crate::tui::input::fixtures::key(KeyCode::Char('e')),
            dir,
        );
        assert!(
            state.drain_effects().iter().any(|effect| matches!(
                effect,
                ManagerEffect::Workflows(WorkflowsEffect::Export { out_dir, .. })
                if out_dir == "/tmp/workflow-exports"
            )),
            "export must carry the host-owned directory"
        );
    }

    #[test]
    fn escape_closes_detail_then_returns_to_list() {
        let mut state = entered_state();
        let dir = std::path::Path::new("/tmp/exports");
        let key = crate::tui::input::fixtures::key;
        handle_key(&mut state, key(KeyCode::Esc), dir);
        let ManagerStage::Workflows(screen) = &state.stage else {
            panic!("first Esc must only close the detail");
        };
        assert!(screen.detail().is_none());
        handle_key(&mut state, key(KeyCode::Esc), dir);
        assert!(matches!(state.stage, ManagerStage::List));
    }
}
