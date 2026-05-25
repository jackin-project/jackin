use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::console::widgets::error_popup::{self, ErrorPopupState};
use crate::console::widgets::{
    PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, render_brand_header,
};
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
    /// Mounts whose host source differs from the container destination,
    /// pre-formatted for display. Same-path mounts are omitted upstream.
    pub mounts: Vec<String>,
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
    /// Eased progress fraction [0,1] driving the rail's green sweep. Lerps
    /// toward `fill_target` each frame so completed stages flow the fill
    /// forward smoothly instead of snapping.
    fill_shown: f32,
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
                fill_shown: 0.0,
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
        self.advance_fill();
        if let Renderer::Rich(renderer) = &mut self.renderer {
            let _ = renderer.render(&self.view, self.diagnostics.run_id());
        }
    }

    fn advance_fill(&mut self) {
        let target = fill_target(&self.view);
        if self.no_motion() {
            self.view.fill_shown = target;
            return;
        }
        let delta = target - self.view.fill_shown;
        if delta.abs() < 0.005 {
            self.view.fill_shown = target;
        } else {
            self.view.fill_shown += delta * 0.28;
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

const RAIL_CONNECTOR_CELLS: usize = 3;
const RAIL_PULSE_PERIOD: usize = 5;
const RAIL_ELLIPSIS_PERIOD: usize = 3;
/// Error accent for a failed stage marker. Matches `error_popup`'s
/// private `DANGER_RED`; the launch rail is the only other site that
/// needs the colour, so it is duplicated here rather than made public.
const FAILED_RED: Color = Color::Rgb(255, 94, 122);

fn render_launch_frame(frame: &mut Frame<'_>, view: &LaunchView, run_id: &str, no_motion: bool) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // brand header (pill + spacer) — shared chrome
            Constraint::Min(8),    // launch body
            Constraint::Length(1), // status / diagnostics
        ])
        .split(area);

    // Freeze animated accents while a failure popup owns the screen so no
    // live cue keeps moving behind the modal.
    let frozen = no_motion || view.failure.is_some();

    render_brand_header(frame, rows[0], "launch");
    render_body(frame, rows[1], view, frozen);
    render_footer(frame, rows[2], view, run_id);

    if let Some(failure) = &view.failure {
        render_failure_popup(frame, area, failure, run_id);
    }
}

fn box_title(view: &LaunchView) -> String {
    view.identity.as_ref().map_or_else(
        || "Preparing launch".to_string(),
        |id| {
            format!(
                "Loading {} {} {}",
                id.role,
                match id.target_kind {
                    LaunchTargetKind::Workspace => "into",
                    LaunchTargetKind::Directory => "in",
                },
                id.target_label
            )
        },
    )
}

fn render_body(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            format!(" {} ", box_title(view)),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area).inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    frame.render_widget(block, area);

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(identity_height(view.identity.as_ref())),
            Constraint::Min(3),
        ])
        .split(inner);

    render_identity(frame, parts[0], view.identity.as_ref());
    render_rail(frame, parts[1], view, frozen);
}

fn identity_height(identity: Option<&LaunchIdentity>) -> u16 {
    identity.map_or(1, |id| {
        2 + u16::try_from(id.mounts.len()).unwrap_or(u16::MAX)
            + u16::from(id.image.is_some())
            + u16::from(id.container.is_some())
    })
}

fn render_identity(frame: &mut Frame<'_>, area: Rect, identity: Option<&LaunchIdentity>) {
    let Some(id) = identity else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "resolving launch identity",
                Style::default().fg(PHOSPHOR_DIM),
            ))),
            area,
        );
        return;
    };
    let mut lines = vec![
        identity_line("agent", &id.agent),
        identity_line("workdir", &id.workdir),
    ];
    for (i, mount) in id.mounts.iter().enumerate() {
        let label = if i > 0 {
            "" // continuation rows align under the first mount
        } else if id.mounts.len() == 1 {
            "mount"
        } else {
            "mounts"
        };
        lines.push(identity_line(label, mount));
    }
    if let Some(image) = &id.image {
        lines.push(identity_line("image", image));
    }
    if let Some(container) = &id.container {
        lines.push(identity_line("container", container));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn identity_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<10}"), Style::default().fg(PHOSPHOR_DIM)),
        Span::styled(value.to_string(), Style::default().fg(WHITE)),
    ])
}

fn render_rail(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let lines = vec![
        markers_line(view, frozen),
        Line::raw(""),
        labels_line(view, frozen),
        detail_line(view, frozen),
    ];
    // Vertically centre the rail within its area so the focal stage sits
    // in the middle of the box, not pinned to the top.
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let top = area.y + area.height.saturating_sub(height) / 2;
    let rect = Rect {
        x: area.x,
        y: top,
        width: area.width,
        height: height.min(area.height),
    };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rect);
}

fn markers_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let connector_cells = view.stages.len().saturating_sub(1) * RAIL_CONNECTOR_CELLS;
    let front = (view.fill_shown * connector_cells as f32).round() as usize;
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut cell = 0usize;
    for (i, row) in view.stages.iter().enumerate() {
        if i > 0 {
            for _ in 0..RAIL_CONNECTOR_CELLS {
                let color = if cell < front {
                    PHOSPHOR_GREEN
                } else {
                    PHOSPHOR_DARK
                };
                spans.push(Span::styled("─", Style::default().fg(color)));
                cell += 1;
            }
        }
        spans.push(marker_span(row.status, view.frame, frozen));
    }
    Line::from(spans)
}

fn marker_span(status: StageStatus, frame: usize, frozen: bool) -> Span<'static> {
    let bright = !frozen && (frame / RAIL_PULSE_PERIOD).is_multiple_of(2);
    match status {
        StageStatus::Running => Span::styled(
            "◉",
            if bright {
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            },
        ),
        StageStatus::Done => Span::styled("●", Style::default().fg(PHOSPHOR_GREEN)),
        StageStatus::Skipped => Span::styled("◌", Style::default().fg(PHOSPHOR_DIM)),
        StageStatus::Failed => Span::styled("✕", Style::default().fg(FAILED_RED)),
        StageStatus::Blocked => Span::styled("◈", Style::default().fg(WHITE)),
        StageStatus::Queued => Span::styled("○", Style::default().fg(PHOSPHOR_DARK)),
    }
}

fn labels_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let active = active_stage_index(view);
    let bright = !frozen && (view.frame / RAIL_PULSE_PERIOD).is_multiple_of(2);
    let active_style = if bright {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    // Just-completed stage to the left (dim), focal stage in the middle
    // (bright), queued stage to the right (dark): the operator reads where
    // they came from, where they are, and where they are going.
    if active > 0 {
        spans.push(Span::styled(
            view.stages[active - 1].stage.label().to_string(),
            Style::default().fg(PHOSPHOR_DIM),
        ));
        spans.push(Span::raw("    "));
    }
    spans.push(Span::styled(
        view.stages[active].stage.label().to_string(),
        active_style,
    ));
    if active + 1 < view.stages.len() {
        spans.push(Span::raw("    "));
        spans.push(Span::styled(
            view.stages[active + 1].stage.label().to_string(),
            Style::default().fg(PHOSPHOR_DARK),
        ));
    }
    Line::from(spans)
}

fn detail_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let row = &view.stages[active_stage_index(view)];
    let text = if row.status == StageStatus::Running {
        let base = row
            .detail
            .trim_end()
            .trim_end_matches('…')
            .trim_end_matches("...")
            .trim_end();
        format!("{base}{}", running_ellipsis(view.frame, frozen))
    } else {
        row.detail.clone()
    };
    Line::from(Span::styled(text, Style::default().fg(PHOSPHOR_DIM)))
}

const fn running_ellipsis(frame: usize, frozen: bool) -> &'static str {
    if frozen {
        return "…";
    }
    // Stable 3-cell width so centring does not jitter as the dots cycle.
    ["   ", ".  ", ".. ", "..."][(frame / RAIL_ELLIPSIS_PERIOD) % 4]
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

fn fill_target(view: &LaunchView) -> f32 {
    let total = view.stages.len().max(1) as f32;
    let done = view
        .stages
        .iter()
        .filter(|row| matches!(row.status, StageStatus::Done | StageStatus::Skipped))
        .count() as f32;
    // The running stage pulls the fill halfway into its segment so the
    // sweep reads as "arriving at" the active marker, not stopped behind it.
    let active = if view.stages.iter().any(|row| row.status == StageStatus::Running) {
        0.5
    } else {
        0.0
    };
    ((done + active) / total).clamp(0.0, 1.0)
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, run_id: &str) {
    let diagnostics = format!("diagnostics · {run_id}");
    let diag_w = u16::try_from(diagnostics.chars().count()).unwrap_or(u16::MAX);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(diag_w.saturating_add(1)),
        ])
        .split(area);

    let status = Line::from(vec![
        Span::styled("status", Style::default().fg(PHOSPHOR_DIM)),
        Span::styled(" · ", Style::default().fg(PHOSPHOR_DARK)),
        Span::styled(view.status.clone(), Style::default().fg(WHITE)),
    ]);
    frame.render_widget(Paragraph::new(status), cols[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            diagnostics,
            Style::default().fg(PHOSPHOR_DARK),
        )))
        .alignment(Alignment::Right),
        cols[1],
    );
}

fn render_failure_popup(frame: &mut Frame<'_>, area: Rect, failure: &LaunchFailure, run_id: &str) {
    let next = failure
        .next_step
        .as_deref()
        .map(|next| format!("\n\n{next}"))
        .unwrap_or_default();
    let message = format!(
        "{}\n\nstage · {}{next}\n\ndiagnostics · {run_id}",
        failure.summary,
        failure.stage.label(),
    );

    let state = ErrorPopupState::new(failure.title.clone(), message);
    let popup_w = (area.width.saturating_mul(3) / 5)
        .clamp(40.min(area.width), area.width.saturating_sub(2).max(1));
    let inner_w = popup_w.saturating_sub(4).max(1);
    let height = error_popup::required_height(&state, inner_w, area.height.saturating_sub(2));
    let rect = centered_rect(popup_w, height, area);
    error_popup::render(frame, rect, &state);
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
                mounts: vec!["~/big-monorepo → /workspace".to_string()],
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
            fill_shown: 0.3,
        };
        terminal
            .draw(|frame| render_launch_frame(frame, &view, "jk-run-42f9aa", true))
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Loading agent-smith into big-monorepo"));
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
        assert!(rendered.contains("docker daemon is not responding"));
        // The reused error_popup carries its own dismiss hint.
        assert!(rendered.contains("Enter/O"));
    }
}
