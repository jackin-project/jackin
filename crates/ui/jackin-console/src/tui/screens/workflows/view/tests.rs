// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_workflow::{RunDetail, RunSummary};

use super::super::model::WorkflowScreen;
use super::{blocking_reason, workflow_detail_lines, workflow_list_line};

fn screen_one() -> WorkflowScreen {
    WorkflowScreen::new(vec![RunSummary {
        run_id: "run-1".to_owned(),
        plan_hash: "plan".to_owned(),
        status: "paused".to_owned(),
        stage: "test".to_owned(),
        round: 1,
        candidate: Some("cand-1".to_owned()),
        candidate_generation: 1,
        version: 2,
        owner_epoch: 1,
        pause_kind: Some("operator".to_owned()),
        max_repair_turns: 3,
    }])
}

#[test]
fn list_line_shows_run_stage_and_status() {
    let screen = screen_one();
    let line = workflow_list_line(0, true, &screen);
    assert!(line.contains("run-1"));
    assert!(line.contains("paused"));
    assert!(line.contains("test"));
    assert!(line.starts_with('>'));
}

#[test]
fn detail_marks_graph_nodes_around_current_stage() {
    let mut screen = screen_one();
    screen.set_detail(RunDetail {
        summary: screen_one().runs()[0].clone(),
        sessions: Vec::new(),
        effects: Vec::new(),
        events: Vec::new(),
        evidence: Vec::new(),
        checkpoints: Vec::new(),
    });
    let lines = workflow_detail_lines(&screen);
    let text = lines.join("\n");
    assert!(text.contains("prepare: done"));
    assert!(text.contains("test: current"));
    assert!(text.contains("review: pending"));
    assert!(text.contains("paused (operator)"));
}

#[test]
fn blocking_reason_names_operator_state() {
    assert_eq!(
        blocking_reason("needs_operator", None),
        "needs operator decision"
    );
}
