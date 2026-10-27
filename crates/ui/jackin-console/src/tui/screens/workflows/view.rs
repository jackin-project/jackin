// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::model::{WorkflowAction, WorkflowScreen};

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
