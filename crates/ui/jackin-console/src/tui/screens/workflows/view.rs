// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workflow list/detail rendering on existing Console widgets: a
//! run list (run ID, stage, status, blocking reason), a detail view
//! (fixed graph, current candidate, findings, backup state), and the
//! shared confirm/error dialogs for resume/cancel/takeover.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::model::{WorkflowAction, WorkflowScreen};

/// One rendered list row (exposed for tests and footer hints).
#[must_use]
pub fn workflow_list_line(index: usize, selected: bool, screen: &WorkflowScreen) -> String {
    let Some(run) = screen.runs().get(index) else {
        return String::new();
    };
    let marker = if selected { ">" } else { " " };
    format!(
        "{marker} {} {} {} round {}/{}",
        run.run_id, run.status, run.stage, run.round, run.max_repair_turns
    )
}

/// Human blocking reason for the list view.
#[must_use]
pub fn blocking_reason(status: &str, pause_kind: Option<&str>) -> String {
    match status {
        "needs_operator" => "needs operator decision".to_owned(),
        "paused" => format!("paused ({})", pause_kind.unwrap_or("operator")),
        "failed" => "run failed".to_owned(),
        "cancelled" => "run cancelled".to_owned(),
        "accepted" => "run accepted".to_owned(),
        _ => status.to_owned(),
    }
}

/// Detail body lines: fixed graph with node states, candidate,
/// sessions, evidence, checkpoints, and backup state. The graph is a
/// view of saved state: it cannot edit the workflow.
#[must_use]
pub fn workflow_detail_lines(screen: &WorkflowScreen) -> Vec<String> {
    let Some(detail) = screen.detail() else {
        return vec!["No run detail open.".to_owned()];
    };
    let summary = &detail.summary;
    let mut lines = vec![
        format!("run: {}", summary.run_id),
        format!("plan: {}", summary.plan_hash),
        format!(
            "status: {}",
            blocking_reason(&summary.status, summary.pause_kind.as_deref())
        ),
        format!("stage: {}", summary.stage),
        format!("round: {}/{}", summary.round, summary.max_repair_turns),
        format!(
            "candidate: {}",
            summary.candidate.as_deref().unwrap_or("none")
        ),
        String::new(),
        "graph:".to_owned(),
    ];
    for stage in jackin_workflow::GRAPH_STAGES {
        lines.push(format!(
            "  {stage}: {}",
            jackin_workflow::graph_node_state(stage, &summary.stage)
        ));
    }
    lines.push(String::new());
    lines.push("sessions:".to_owned());
    if detail.sessions.is_empty() {
        lines.push("  none recorded".to_owned());
    }
    for session in &detail.sessions {
        lines.push(format!(
            "  {}: instance={}, session={}, epoch={}",
            session.worker_id,
            session.instance_id.as_deref().unwrap_or("none"),
            session
                .session_id
                .map_or_else(|| "none".to_owned(), |id| id.to_string()),
            session
                .capsule_epoch
                .map_or_else(|| "none".to_owned(), |epoch| epoch.to_string()),
        ));
    }
    lines.push(String::new());
    lines.push("evidence:".to_owned());
    if detail.evidence.is_empty() {
        lines.push("  none recorded".to_owned());
    }
    for evidence in &detail.evidence {
        lines.push(format!(
            "  {} {}: {} ({})",
            evidence.kind, evidence.candidate, evidence.result, evidence.quality
        ));
    }
    lines.push(String::new());
    lines.push("checkpoints:".to_owned());
    if detail.checkpoints.is_empty() {
        lines.push("  none recorded".to_owned());
    }
    for checkpoint in &detail.checkpoints {
        lines.push(format!(
            "  {}: backup={} path={}",
            checkpoint.candidate, checkpoint.backup_state, checkpoint.local_path
        ));
    }
    if let Some(observing) = screen.observing() {
        lines.push(String::new());
        lines.push(format!(
            "observing {} session {} (read-only)",
            observing.worker_id,
            observing
                .session_id
                .map_or_else(|| "?".to_owned(), |id| id.to_string())
        ));
    }
    lines
}

/// Confirmation prompt text for a destructive workflow action.
#[must_use]
pub fn confirm_prompt(action: WorkflowAction, run_id: &str) -> String {
    match action {
        WorkflowAction::Resume => {
            format!("Resume run {run_id} and return control to the scheduler?")
        }
        WorkflowAction::Cancel => {
            format!("Cancel run {run_id}? Its source stays preserved.")
        }
        WorkflowAction::TakeControl => format!(
            "Take exclusive input ownership of run {run_id}'s implementer session? Automatic input stays rejected while held."
        ),
    }
}

pub fn render_workflow_list(frame: &mut Frame<'_>, area: Rect, screen: &WorkflowScreen) {
    let items: Vec<ListItem<'_>> = screen
        .runs()
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let line = workflow_list_line(index, index == screen.selected_index(), screen);
            let item = ListItem::new(Line::from(line));
            if index == screen.selected_index() {
                item.style(Style::default().add_modifier(Modifier::BOLD))
            } else {
                item
            }
        })
        .collect();
    let title = if screen.runs().is_empty() {
        "Workflows (no runs)"
    } else {
        "Workflows"
    };
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

pub fn render_workflow_detail(frame: &mut Frame<'_>, area: Rect, screen: &WorkflowScreen) {
    let lines: Vec<Line<'_>> = workflow_detail_lines(screen)
        .into_iter()
        .map(Line::from)
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Run detail")),
        area,
    );
}

#[cfg(test)]
mod tests {
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
}
