// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
