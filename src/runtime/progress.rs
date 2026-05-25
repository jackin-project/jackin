use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::diagnostics::RunDiagnostics;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LaunchStage {
    Identity,
    Role,
    Credentials,
    Construct,
    DerivedImage,
    Workspace,
    Network,
    Sidecar,
    Capsule,
    Hardline,
}

impl LaunchStage {
    pub const ALL: [Self; 10] = [
        Self::Identity,
        Self::Role,
        Self::Credentials,
        Self::Construct,
        Self::DerivedImage,
        Self::Workspace,
        Self::Network,
        Self::Sidecar,
        Self::Capsule,
        Self::Hardline,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Role => "role",
            Self::Credentials => "credentials",
            Self::Construct => "construct",
            Self::DerivedImage => "derived image",
            Self::Workspace => "workspace",
            Self::Network => "network",
            Self::Sidecar => "sidecar",
            Self::Capsule => "capsule",
            Self::Hardline => "hardline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    Queued,
    Running,
    Done,
    Skipped,
    Failed,
    Blocked,
}

impl StageStatus {
    const fn marker(self) -> &'static str {
        match self {
            Self::Queued => "○",
            Self::Running => "◐",
            Self::Done => "●",
            Self::Skipped => "◇",
            Self::Failed => "×",
            Self::Blocked => "!",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LaunchIdentity {
    pub role: String,
    pub agent: String,
    pub target_kind: LaunchTargetKind,
    pub target_label: String,
    pub workdir: String,
    pub mount_summary: String,
    pub image: Option<String>,
    pub container: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTargetKind {
    Workspace,
    Directory,
}

impl LaunchTargetKind {
    const fn launch_preposition(self) -> &'static str {
        match self {
            Self::Workspace => "into workspace",
            Self::Directory => "in directory",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Directory => "directory",
        }
    }
}

#[derive(Debug, Clone)]
struct StageView {
    stage: LaunchStage,
    status: StageStatus,
    detail: String,
}

#[derive(Debug, Clone)]
struct LaunchView {
    identity: Option<LaunchIdentity>,
    stages: Vec<StageView>,
    status: String,
    failure: Option<LaunchFailure>,
    frame: usize,
}

#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub title: String,
    pub summary: String,
    pub next_step: Option<String>,
    pub stage: LaunchStage,
}

pub struct LaunchProgress {
    diagnostics: Arc<RunDiagnostics>,
    renderer: Renderer,
    view: LaunchView,
}

enum Renderer {
    Rich(RichRenderer),
    Compact {
        interactive: bool,
    },
    #[cfg(test)]
    Test,
}

impl LaunchProgress {
    pub fn new(
        diagnostics: Arc<RunDiagnostics>,
        no_tui: bool,
        no_motion: bool,
    ) -> anyhow::Result<Self> {
        let renderer = if rich_terminal_supported() && !no_tui {
            Renderer::Rich(RichRenderer::enter(no_motion)?)
        } else {
            Renderer::Compact {
                interactive: std::io::stderr().is_terminal(),
            }
        };
        Ok(Self::with_renderer(diagnostics, renderer))
    }

    #[cfg(test)]
    pub fn for_test(diagnostics: Arc<RunDiagnostics>) -> Self {
        Self::with_renderer(diagnostics, Renderer::Test)
    }

    fn with_renderer(diagnostics: Arc<RunDiagnostics>, renderer: Renderer) -> Self {
        let stages = LaunchStage::ALL
            .into_iter()
            .map(|stage| StageView {
                stage,
                status: StageStatus::Queued,
                detail: "queued".to_string(),
            })
            .collect();
        Self {
            diagnostics,
            renderer,
            view: LaunchView {
                identity: None,
                stages,
                status: "preparing launch".to_string(),
                failure: None,
                frame: 0,
            },
        }
    }

    pub fn run_id(&self) -> &str {
        self.diagnostics.run_id()
    }

    pub fn started(&mut self, identity: LaunchIdentity) {
        self.view.status = format!(
            "loading {} {}",
            identity.role,
            identity.target_kind.launch_preposition()
        );
        self.view.identity = Some(identity);
        self.diagnostics.compact(
            "launch_started",
            &format!("diagnostics: run {}", self.run_id()),
        );
        self.render();
    }

    pub fn update_identity(&mut self, identity: LaunchIdentity) {
        self.view.identity = Some(identity);
        self.render();
    }

    pub fn stage_started(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_stage(stage, StageStatus::Running, &detail);
        self.view.status.clone_from(&detail);
        self.diagnostics
            .stage("stage_started", stage.label(), &detail, None);
        self.render_or_line(stage, StageStatus::Running, &detail);
    }

    pub fn stage_progress(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_stage(stage, StageStatus::Running, &detail);
        self.view.status.clone_from(&detail);
        self.diagnostics
            .stage("stage_progress", stage.label(), &detail, None);
        self.render();
    }

    pub fn stage_done(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_stage(stage, StageStatus::Done, &detail);
        self.diagnostics
            .stage("stage_done", stage.label(), &detail, None);
        self.render_or_line(stage, StageStatus::Done, &detail);
    }

    pub fn stage_skipped(&mut self, stage: LaunchStage, reason: impl Into<String>) {
        let reason = reason.into();
        self.update_stage(stage, StageStatus::Skipped, &reason);
        self.diagnostics
            .stage("stage_skipped", stage.label(), &reason, None);
        self.render_or_line(stage, StageStatus::Skipped, &reason);
    }

    pub fn stage_failed(&mut self, failure: LaunchFailure) {
        self.update_stage(failure.stage, StageStatus::Failed, &failure.summary);
        self.view.status.clone_from(&failure.summary);
        self.diagnostics.stage(
            "stage_failed",
            failure.stage.label(),
            &failure.summary,
            failure.next_step.as_deref(),
        );
        self.view.failure = Some(failure);
        self.render();
        if matches!(self.renderer, Renderer::Rich(_)) && std::io::stdin().is_terminal() {
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
        }
    }

    pub fn opening_hardline(&mut self) {
        self.stage_started(LaunchStage::Hardline, "opening hardline");
    }

    pub async fn while_waiting<T, E, F>(&mut self, future: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        if !matches!(self.renderer, Renderer::Rich(_)) || self.no_motion() {
            return future.await;
        }
        tokio::pin!(future);
        let mut interval = tokio::time::interval(Duration::from_millis(120));
        loop {
            tokio::select! {
                result = &mut future => return result,
                _ = interval.tick() => self.tick(),
            }
        }
    }

    fn update_stage(&mut self, stage: LaunchStage, status: StageStatus, detail: &str) {
        if let Some(row) = self.view.stages.iter_mut().find(|row| row.stage == stage) {
            row.status = status;
            row.detail = detail.to_string();
        }
    }

    fn render_or_line(&mut self, stage: LaunchStage, status: StageStatus, detail: &str) {
        match &mut self.renderer {
            Renderer::Compact { interactive } => {
                let marker = status.marker();
                let label = stage.label();
                if *interactive {
                    eprintln!("  {marker} {label:<13} {detail}");
                } else {
                    eprintln!("{label}: {}", status.label());
                    if !detail.is_empty() {
                        eprintln!("status: {detail}");
                    }
                }
                eprintln!("diagnostics: run {}", self.run_id());
            }
            Renderer::Rich(_) => self.render(),
            #[cfg(test)]
            Renderer::Test => self.render(),
        }
    }

    fn render(&mut self) {
        if let Renderer::Rich(renderer) = &mut self.renderer {
            let _ = renderer.render(&self.view, self.diagnostics.run_id());
        }
    }

    fn tick(&mut self) {
        self.view.frame = self.view.frame.wrapping_add(1);
        self.render();
    }

    const fn no_motion(&self) -> bool {
        matches!(&self.renderer, Renderer::Rich(renderer) if renderer.no_motion)
    }
}

impl Drop for LaunchProgress {
    fn drop(&mut self) {
        if matches!(self.renderer, Renderer::Compact { .. }) {
            eprintln!("diagnostics: run {}", self.run_id());
        }
    }
}

struct RichRenderer {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    no_motion: bool,
}

impl RichRenderer {
    fn enter(no_motion: bool) -> anyhow::Result<Self> {
        let mut stdout = std::io::stdout();
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(crossterm::cursor::Hide)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = ratatui::Terminal::new(backend)?;
        Ok(Self {
            terminal,
            no_motion,
        })
    }

    fn render(&mut self, view: &LaunchView, run_id: &str) -> anyhow::Result<()> {
        let no_motion = self.no_motion;
        self.terminal
            .draw(|frame| render_launch_frame(frame, view, run_id, no_motion))
            .map(|_| ())
            .context("rendering launch progress TUI")
    }
}

impl Drop for RichRenderer {
    fn drop(&mut self) {
        let _ = self.terminal.backend_mut().execute(crossterm::cursor::Show);
        let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        let _ = std::io::stdout().flush();
    }
}

fn rich_terminal_supported() -> bool {
    if !std::io::stdout().is_terminal() || !std::io::stderr().is_terminal() {
        return false;
    }
    if std::env::var_os("CI").is_some() {
        return false;
    }
    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return false;
    }
    crossterm::terminal::size().is_ok_and(|(cols, rows)| cols >= 80 && rows >= 24)
}

fn render_launch_frame(frame: &mut Frame<'_>, view: &LaunchView, run_id: &str, no_motion: bool) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" jackin' / launch ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    frame.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(inner);
    render_identity(frame, rows[0], view.identity.as_ref());
    render_stages(frame, rows[1], view, no_motion);
    render_footer(frame, rows[2], view, run_id, no_motion);
    if let Some(failure) = &view.failure {
        render_failure(frame, centered_rect(58, 8, area), failure, run_id);
    }
}

fn render_identity(frame: &mut Frame<'_>, area: Rect, identity: Option<&LaunchIdentity>) {
    let Some(identity) = identity else {
        frame.render_widget(Paragraph::new("resolving launch identity"), area);
        return;
    };
    let mut lines = vec![Line::from(vec![Span::styled(
        format!(
            "jackin' / loading {} {}",
            identity.role,
            identity.target_kind.launch_preposition()
        ),
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )])];
    lines.push(Line::from(format!(
        "{:<13} {}",
        identity.target_kind.label(),
        identity.target_label
    )));
    lines.push(Line::from(format!(
        "{:<13} {}",
        "workdir", identity.workdir
    )));
    lines.push(Line::from(format!(
        "{:<13} {}",
        if identity.target_kind == LaunchTargetKind::Workspace {
            "mounts"
        } else {
            "mount"
        },
        identity.mount_summary
    )));
    lines.push(Line::from(format!("{:<13} {}", "agent", identity.agent)));
    if let Some(container) = &identity.container {
        lines.push(Line::from(format!("{:<13} {container}", "container")));
    }
    if let Some(image) = &identity.image {
        lines.push(Line::from(format!("{:<13} {image}", "image")));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_stages(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, no_motion: bool) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);
    let rain = if no_motion {
        "│\n│\n│\n│\n│".to_string()
    } else {
        let frames = [
            "╷\n│\n╵\n│\n╷",
            "│\n╷\n│\n╵\n│",
            "╵\n│\n╷\n│\n╵",
            "│\n╵\n│\n╷\n│",
        ];
        frames[view.frame % frames.len()].to_string()
    };
    frame.render_widget(
        Paragraph::new(rain).style(Style::default().fg(Color::DarkGray)),
        chunks[0],
    );
    let stage_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(chunks[1]);
    render_stage_slide(frame, stage_area[0], view, no_motion);
    let lines: Vec<Line<'_>> = view
        .stages
        .iter()
        .map(|row| {
            let style = match row.status {
                StageStatus::Queued => Style::default().fg(Color::DarkGray),
                StageStatus::Running => Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                StageStatus::Done => Style::default().fg(Color::Green),
                StageStatus::Skipped => Style::default().fg(Color::Yellow),
                StageStatus::Failed => Style::default().fg(Color::Red),
                StageStatus::Blocked => Style::default().fg(Color::Magenta),
            };
            Line::from(vec![
                Span::styled(
                    format!("{} ", status_marker(row.status, view.frame, no_motion)),
                    style,
                ),
                Span::styled(format!("{:<13} ", row.stage.label()), style),
                Span::raw(row.detail.clone()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), stage_area[1]);
}

fn render_stage_slide(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, no_motion: bool) {
    let current = active_stage_index(view);
    let start = current.saturating_sub(2);
    let end = (start + 5).min(view.stages.len());
    let mut rail: Vec<Span<'_>> = Vec::new();
    for (offset, row) in view.stages[start..end].iter().enumerate() {
        if offset > 0 {
            rail.push(Span::styled("  ->  ", Style::default().fg(Color::DarkGray)));
        }
        let index = start + offset;
        let style = if index == current {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            match row.status {
                StageStatus::Done => Style::default().fg(Color::Green),
                StageStatus::Skipped => Style::default().fg(Color::Yellow),
                StageStatus::Failed => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::DarkGray),
            }
        };
        let marker = status_marker(row.status, view.frame + offset, no_motion);
        rail.push(Span::styled(
            format!("{marker} {}", row.stage.label()),
            style,
        ));
    }
    let current_row = &view.stages[current];
    let pulse = if no_motion {
        "waiting"
    } else {
        const PULSE: [&str; 4] = ["waiting", "waiting.", "waiting..", "waiting..."];
        PULSE[view.frame % PULSE.len()]
    };
    let lines = vec![
        Line::from(rail),
        Line::from(vec![Span::styled(
            format!(
                "{}  {}",
                current_row.stage.label(),
                if current_row.status == StageStatus::Running {
                    pulse
                } else {
                    current_row.status.label()
                }
            ),
            Style::default().fg(Color::White),
        )]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn active_stage_index(view: &LaunchView) -> usize {
    view.stages
        .iter()
        .position(|row| row.status == StageStatus::Running)
        .or_else(|| {
            view.stages
                .iter()
                .rposition(|row| matches!(row.status, StageStatus::Done | StageStatus::Skipped))
        })
        .unwrap_or(0)
}

fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    no_motion: bool,
) {
    let hint = if view.failure.is_some() {
        "Enter Close"
    } else {
        "opening hardline when ready"
    };
    let line = Line::from(vec![
        Span::styled(
            format!(
                "{} ",
                status_marker(StageStatus::Running, view.frame, no_motion)
            ),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled("status: ", Style::default().fg(Color::Green)),
        Span::raw(&view.status),
        Span::raw("    "),
        Span::styled(hint, Style::default().fg(Color::White)),
        Span::raw("    "),
        Span::styled(
            format!("diagnostics: run {run_id}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn status_marker(status: StageStatus, frame: usize, no_motion: bool) -> &'static str {
    if status == StageStatus::Running && !no_motion {
        const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
        FRAMES[frame % FRAMES.len()]
    } else {
        status.marker()
    }
}

fn render_failure(frame: &mut Frame<'_>, area: Rect, failure: &LaunchFailure, run_id: &str) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(format!(" {} ", failure.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    frame.render_widget(block, area);
    let mut lines = vec![
        Line::from(failure.summary.clone()),
        Line::from(format!("stage: {}", failure.stage.label())),
    ];
    if let Some(next) = &failure.next_step {
        lines.push(Line::from(next.clone()));
    }
    lines.push(Line::from(format!("diagnostics: run {run_id}")));
    lines.push(Line::from("Enter Close"));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn stage_labels_are_stable() {
        let labels: Vec<&str> = LaunchStage::ALL.iter().map(|stage| stage.label()).collect();
        assert_eq!(
            labels,
            vec![
                "identity",
                "role",
                "credentials",
                "construct",
                "derived image",
                "workspace",
                "network",
                "sidecar",
                "capsule",
                "hardline"
            ]
        );
    }

    #[test]
    fn rich_renderer_frame_contains_identity_stages_and_diagnostics() {
        let backend = TestBackend::new(120, 28);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut view = LaunchView {
            identity: Some(LaunchIdentity {
                role: "agent-smith".to_string(),
                agent: "claude".to_string(),
                target_kind: LaunchTargetKind::Workspace,
                target_label: "big-monorepo".to_string(),
                workdir: "~/Projects/app".to_string(),
                mount_summary: "3 configured".to_string(),
                image: Some("jk_agent-smith:latest".to_string()),
                container: Some("jk-k7p9m2xq-bigmonorepo-agentsmith".to_string()),
            }),
            stages: LaunchStage::ALL
                .into_iter()
                .map(|stage| StageView {
                    stage,
                    status: if stage == LaunchStage::Construct {
                        StageStatus::Running
                    } else {
                        StageStatus::Queued
                    },
                    detail: if stage == LaunchStage::Construct {
                        "pulling construct".to_string()
                    } else {
                        "queued".to_string()
                    },
                })
                .collect(),
            status: "pulling construct".to_string(),
            failure: None,
            frame: 0,
        };
        terminal
            .draw(|frame| render_launch_frame(frame, &view, "jk-run-42f9aa", true))
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("loading agent-smith into workspace"));
        assert!(rendered.contains("construct"));
        assert!(rendered.contains("jk-run-42f9aa"));

        view.failure = Some(LaunchFailure {
            title: "Docker unavailable".to_string(),
            summary: "docker daemon is not responding".to_string(),
            next_step: Some("Start Docker and run the command again.".to_string()),
            stage: LaunchStage::Network,
        });
        terminal
            .draw(|frame| render_launch_frame(frame, &view, "jk-run-42f9aa", true))
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Docker unavailable"));
        assert!(rendered.contains("Enter Close"));
    }
}
