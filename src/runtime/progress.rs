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
use ratatui::widgets::{Clear, Paragraph};

use crate::console::widgets::error_popup::{self, ErrorPopupState};
use crate::console::widgets::select_list::{self, SelectListState};
use crate::console::widgets::{
    ModalOutcome, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE,
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
}

#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub title: String,
    pub summary: String,
    pub next_step: Option<String>,
    pub stage: LaunchStage,
}

type SharedView = Arc<std::sync::Mutex<LaunchView>>;

pub struct LaunchProgress {
    diagnostics: Arc<RunDiagnostics>,
    renderer: Renderer,
    view: SharedView,
}

enum Renderer {
    Rich(RichDriver),
    Compact {
        interactive: bool,
    },
    /// Rich surface torn down at the handoff; inert (no draws, no diagnostics
    /// trailer) so the interactive capsule attach owns the terminal alone.
    Done,
    #[cfg(test)]
    Test,
}

/// Owns the background render task that ticks the cockpit independently of the
/// launch work, so the rain and animation never freeze while a launch step is
/// blocked on I/O. The task shares the renderer behind a `try_lock` (so the
/// reclaiming picker is never blocked) and a stop flag.
struct RichDriver {
    renderer: Arc<std::sync::Mutex<RichRenderer>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl RichDriver {
    fn spawn(renderer: RichRenderer, view: SharedView, run_id: String) -> Self {
        use std::sync::atomic::Ordering;
        let renderer = Arc::new(std::sync::Mutex::new(renderer));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = {
            let renderer = renderer.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(33));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    // Try-lock so a picker reclaiming the renderer is never
                    // blocked; snapshot the view (advancing the animation frame)
                    // without holding the view lock across the draw.
                    let Ok(mut rr) = renderer.try_lock() else {
                        continue;
                    };
                    let snapshot = match view.lock() {
                        Ok(mut v) => {
                            if !rr.no_motion {
                                v.frame = v.frame.wrapping_add(1);
                            }
                            v.clone()
                        }
                        Err(_) => continue,
                    };
                    let _ = rr.render(&snapshot, &run_id);
                }
            })
        };
        Self {
            renderer,
            stop,
            handle: Some(handle),
        }
    }
}

fn initial_view() -> LaunchView {
    LaunchView {
        identity: None,
        stages: LaunchStage::ALL
            .into_iter()
            .map(|stage| StageView {
                stage,
                status: StageStatus::Queued,
                detail: "queued".to_string(),
            })
            .collect(),
        status: "preparing launch".to_string(),
        failure: None,
        frame: 0,
    }
}

impl LaunchProgress {
    pub fn new(
        diagnostics: Arc<RunDiagnostics>,
        no_tui: bool,
        no_motion: bool,
    ) -> anyhow::Result<Self> {
        let view: SharedView = Arc::new(std::sync::Mutex::new(initial_view()));
        let renderer = if rich_terminal_supported() && !no_tui {
            let rich = RichRenderer::enter(no_motion)?;
            Renderer::Rich(RichDriver::spawn(
                rich,
                view.clone(),
                diagnostics.run_id().to_string(),
            ))
        } else {
            Renderer::Compact {
                interactive: std::io::stderr().is_terminal(),
            }
        };
        Ok(Self {
            diagnostics,
            renderer,
            view,
        })
    }

    #[cfg(test)]
    pub fn for_test(diagnostics: Arc<RunDiagnostics>) -> Self {
        Self {
            diagnostics,
            renderer: Renderer::Test,
            view: Arc::new(std::sync::Mutex::new(initial_view())),
        }
    }

    pub fn run_id(&self) -> &str {
        self.diagnostics.run_id()
    }

    /// Mutate the shared view; the background render task redraws it on its next
    /// tick (≤33ms), so callers never block on drawing.
    fn with_view(&self, f: impl FnOnce(&mut LaunchView)) {
        if let Ok(mut view) = self.view.lock() {
            f(&mut view);
        }
    }

    pub fn started(&mut self, identity: LaunchIdentity) {
        let preposition = identity.target_kind.launch_preposition();
        self.with_view(|v| {
            v.status = format!("loading {} {preposition}", identity.role);
            v.identity = Some(identity);
        });
        self.diagnostics.compact(
            "launch_started",
            &format!("diagnostics: run {}", self.run_id()),
        );
    }

    pub fn update_identity(&mut self, identity: LaunchIdentity) {
        self.with_view(|v| v.identity = Some(identity));
    }

    pub fn stage_started(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.with_view(|v| {
            update_stage(v, stage, StageStatus::Running, &detail);
            v.status.clone_from(&detail);
        });
        self.diagnostics
            .stage("stage_started", stage.label(), &detail, None);
        self.compact_line(stage, StageStatus::Running, &detail);
    }

    pub fn stage_progress(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.with_view(|v| {
            update_stage(v, stage, StageStatus::Running, &detail);
            v.status.clone_from(&detail);
        });
        self.diagnostics
            .stage("stage_progress", stage.label(), &detail, None);
    }

    pub fn stage_done(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.with_view(|v| update_stage(v, stage, StageStatus::Done, &detail));
        self.diagnostics
            .stage("stage_done", stage.label(), &detail, None);
        self.compact_line(stage, StageStatus::Done, &detail);
    }

    pub fn stage_skipped(&mut self, stage: LaunchStage, reason: impl Into<String>) {
        let reason = reason.into();
        self.with_view(|v| update_stage(v, stage, StageStatus::Skipped, &reason));
        self.diagnostics
            .stage("stage_skipped", stage.label(), &reason, None);
        self.compact_line(stage, StageStatus::Skipped, &reason);
    }

    pub fn stage_failed(&mut self, failure: LaunchFailure) {
        let stage = failure.stage;
        let summary = failure.summary.clone();
        let next_step = failure.next_step.clone();
        self.with_view(|v| {
            update_stage(v, stage, StageStatus::Failed, &summary);
            v.status.clone_from(&summary);
            v.failure = Some(failure);
        });
        self.diagnostics
            .stage("stage_failed", stage.label(), &summary, next_step.as_deref());
        // The render task draws the failure popup; wait for the operator to
        // acknowledge it on a rich terminal.
        if matches!(self.renderer, Renderer::Rich(_)) && std::io::stdin().is_terminal() {
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
        }
    }

    pub fn opening_hardline(&mut self) {
        self.stage_started(LaunchStage::Hardline, "opening hardline");
    }

    /// Stop the render task and release the rich surface before the interactive
    /// handoff, so the capsule attach owns the terminal alone. Idempotent;
    /// no-op for the compact and test renderers.
    pub fn finish(&mut self) {
        use std::sync::atomic::Ordering;
        if let Renderer::Rich(driver) = &mut self.renderer {
            // Signal the task to stop drawing; it exits on its next tick and
            // drops its renderer (any stray final frame is wiped by the
            // capsule's clear-on-attach). Detach the handle — we do not block.
            driver.stop.store(true, Ordering::Relaxed);
            let _ = driver.handle.take();
            // The interactive attach must inherit the terminal, not be
            // captured, so clear the rich-surface flag now regardless of when
            // the task's renderer finally drops.
            crate::tui::set_rich_surface_active(false);
            self.renderer = Renderer::Done;
        }
    }

    fn compact_line(&self, stage: LaunchStage, status: StageStatus, detail: &str) {
        if let Renderer::Compact { interactive } = &self.renderer {
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
    }

    /// Present a forced-choice picker over `items` and return the chosen
    /// index. Returns `Ok(None)` when no rich surface is active, so the
    /// caller can fall back to the plain stdin prompt. The picker cannot
    /// be cancelled — the operator must commit one of the options.
    pub fn select_choice(
        &mut self,
        title: &str,
        items: Vec<String>,
    ) -> anyhow::Result<Option<usize>> {
        let run_id = self.diagnostics.run_id().to_string();
        if let Renderer::Rich(driver) = &mut self.renderer {
            // Reclaim the renderer from the render task for the modal picker.
            // The task try-locks, so it simply skips frames while we hold it.
            let mut renderer = driver
                .renderer
                .lock()
                .map_err(|_| anyhow::anyhow!("launch renderer mutex poisoned"))?;
            let view = self
                .view
                .lock()
                .map_err(|_| anyhow::anyhow!("launch view mutex poisoned"))?
                .clone();
            renderer.select(&view, &run_id, title, items).map(Some)
        } else {
            Ok(None)
        }
    }

    #[allow(clippy::unused_self)]
    pub async fn while_waiting<T, E, F>(&self, future: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        // The background render task ticks the cockpit independently, so the
        // awaited work no longer needs to interleave a draw — just await it.
        future.await
    }
}

fn update_stage(view: &mut LaunchView, stage: LaunchStage, status: StageStatus, detail: &str) {
    if let Some(row) = view.stages.iter_mut().find(|row| row.stage == stage) {
        row.status = status;
        row.detail = detail.to_string();
    }
}

impl Drop for LaunchProgress {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        // Dropped without an explicit finish (e.g. an error path): stop the
        // render task. Its renderer drops when the task exits, restoring the
        // terminal — the host-screen guard is the ultimate safety net.
        if let Renderer::Rich(driver) = &self.renderer {
            driver.stop.store(true, Ordering::Relaxed);
        }
        // Non-rich launches print the run-id trailer on completion.
        if matches!(self.renderer, Renderer::Compact { .. }) {
            eprintln!("diagnostics: run {}", self.run_id());
        }
    }
}

struct RichRenderer {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    no_motion: bool,
    /// Shared digital-rain engine (the same one the intro/outro use), ticked
    /// per frame and painted into the loading box. Sized to the terminal so
    /// the box shows a window into one continuous rainfall.
    rain: Option<crate::tui::animation::RainState>,
}

impl RichRenderer {
    fn enter(no_motion: bool) -> anyhow::Result<Self> {
        let mut stdout = std::io::stdout();
        // When the launch flow's host guard already owns the alternate screen,
        // draw into it; only enter it ourselves when running standalone.
        if !crate::tui::host_screen_owned() {
            stdout.execute(EnterAlternateScreen)?;
        }
        stdout.execute(crossterm::cursor::Hide)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = ratatui::Terminal::new(backend)?;
        // Wipe whatever the previous surface left on the screen and force a full
        // first redraw. Under the host guard we skipped EnterAlternateScreen
        // (which would have cleared), so the console's last frame is still on
        // the inherited screen — clear it or the cockpit renders over it.
        terminal.clear().context("clearing launch screen")?;
        // Ancillary status printers (spinners) go silent while this surface
        // owns the alternate screen.
        crate::tui::set_rich_surface_active(true);
        Ok(Self {
            terminal,
            no_motion,
            rain: None,
        })
    }

    fn render(&mut self, view: &LaunchView, run_id: &str) -> anyhow::Result<()> {
        let no_motion = self.no_motion;
        // Keep the rain engine sized to the terminal. Advance it every other
        // render so the rainfall reads at the calmer main-branch speed while
        // the frame still redraws smoothly (~30fps). Paused under no-motion.
        if let Ok(size) = self.terminal.size() {
            let (cols, rows) = (size.width as usize, size.height as usize);
            let stale = self
                .rain
                .as_ref()
                .is_none_or(|rain| rain.cols != cols || rain.rows != rows);
            if stale && cols > 0 && rows > 0 {
                self.rain = Some(crate::tui::animation::RainState::new(cols, rows));
            }
            if !no_motion
                && !view.frame.is_multiple_of(3)
                && let Some(rain) = &mut self.rain
            {
                crate::tui::animation::tick_rain(rain);
            }
        }
        let rain = self.rain.as_ref();
        self.terminal
            .draw(|frame| render_launch_frame(frame, view, run_id, no_motion, rain))
            .map(|_| ())
            .context("rendering launch progress TUI")
    }

    /// Run a forced-choice picker over the dimmed launch frame. Enables
    /// raw mode for the duration so key events arrive un-buffered, and
    /// restores it on every exit path. `Ctrl-C` aborts the launch.
    fn select(
        &mut self,
        view: &LaunchView,
        run_id: &str,
        title: &str,
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        // The host guard already holds raw mode for the whole flow; only
        // toggle it when this renderer is running standalone.
        let owns_raw = !crate::tui::host_screen_owned();
        if owns_raw {
            crossterm::terminal::enable_raw_mode().context("entering raw mode for launch picker")?;
        }
        let outcome = self.select_loop(view, run_id, title, items);
        if owns_raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        outcome
    }

    fn select_loop(
        &mut self,
        view: &LaunchView,
        run_id: &str,
        title: &str,
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
        let mut picker = SelectListState::new(items);
        loop {
            self.terminal
                .draw(|frame| draw_select(frame, view, run_id, title, &picker))
                .context("rendering launch picker")?;
            if let Event::Key(key) = crossterm::event::read().context("reading launch picker input")?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    anyhow::bail!("launch cancelled by operator");
                }
                // Esc reports Cancel; ignored here so the choice is forced.
                if let ModalOutcome::Commit(index) = picker.handle_key(key) {
                    return Ok(index);
                }
            }
        }
    }
}

impl Drop for RichRenderer {
    fn drop(&mut self) {
        crate::tui::set_rich_surface_active(false);
        let _ = self.terminal.backend_mut().execute(crossterm::cursor::Show);
        // Leave the alternate screen only when we entered it; under the host
        // guard the screen persists into the capsule attach.
        if !crate::tui::host_screen_owned() {
            let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        }
        let _ = std::io::stdout().flush();
    }
}

pub(crate) fn rich_terminal_supported() -> bool {
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

const STAGE_PULSE_PERIOD: usize = 12;
const BLOCK_WIDTH: usize = 3;
const BLOCK_GAP: usize = 1;
/// Error accent for a failed stage block. Matches `error_popup`'s private
/// `DANGER_RED`; the launch screen is the only other site that needs the
/// colour, so it is duplicated here rather than made public.
const FAILED_RED: Color = Color::Rgb(255, 94, 122);

fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    no_motion: bool,
    rain: Option<&crate::tui::animation::RainState>,
) {
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

    render_cockpit_header(frame, rows[0], view, frozen);
    render_body(frame, rows[1], view, frozen, rain);
    render_footer(frame, rows[2], view, run_id);

    if let Some(failure) = &view.failure {
        render_failure_popup(frame, area, failure, run_id);
    }
}

fn render_body(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    frozen: bool,
    rain: Option<&crate::tui::animation::RainState>,
) {
    // No border — the rain fills the whole body; a one-cell side margin keeps
    // glyphs off the screen edge.
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
    // Digital rain fills the space; the block progress + stage words sit above
    // a blank gap so the bar does not stick to the status bar.
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // rain
            Constraint::Length(2), // progress blocks + stage words
            Constraint::Length(2), // gap above the status bar
        ])
        .split(inner);
    render_rain(frame, parts[0], rain);
    render_progress(frame, parts[1], view, frozen);
}

/// Paint the shared rain engine's grid into `area`. The grid is sized to the
/// whole terminal, so `area` is a window onto a continuous rainfall; each cell
/// maps to its glyph and the engine's green age fade — the same palette as the
/// intro/outro rain.
fn render_rain(frame: &mut Frame<'_>, area: Rect, rain: Option<&crate::tui::animation::RainState>) {
    let Some(rain) = rain else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Fade the whole field up from black over the first ~30 ticks so the rain
    // eases in smoothly instead of popping on at full brightness.
    let fade_in = (rain.frame as f32 / 30.0).min(1.0);
    // Fade the rain to black over the bottom rows so it dissolves into a gap
    // above the progress bar instead of colliding with it: the bottommost row
    // is fully extinguished and brightness ramps back to full a few rows up.
    let fade_rows = (area.height / 3).clamp(3, 7);
    let lines: Vec<Line<'static>> = (0..area.height)
        .map(|y| {
            let grid_y = usize::from(area.y + y);
            let rows_from_bottom = area.height - 1 - y;
            let fade = if rows_from_bottom >= fade_rows {
                1.0
            } else {
                f32::from(rows_from_bottom) / f32::from(fade_rows)
            };
            let dim = |c: u8| (f32::from(c) * fade * fade_in) as u8;
            let spans: Vec<Span<'static>> = (0..area.width)
                .map(|x| {
                    let grid_x = usize::from(area.x + x);
                    rain.grid
                        .get(grid_y)
                        .and_then(|row| row.get(grid_x))
                        .and_then(|cell| cell.as_ref())
                        .and_then(|cell| {
                            crate::tui::animation::age_to_color(cell.age).map(|rgb| (cell.ch, rgb))
                        })
                        .map_or_else(
                            || Span::raw(" "),
                            |(ch, (r, g, b))| {
                                Span::styled(
                                    ch.to_string(),
                                    Style::default().fg(Color::Rgb(dim(r), dim(g), dim(b))),
                                )
                            },
                        )
                })
                .collect();
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Top header: the ` jackin' ` brand pill, a separator, then the loading line
/// (`Loading <role> in <path>`) — replacing both the old brand-header label and
/// the box title.
fn render_cockpit_header(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let mut spans = vec![
        Span::styled(
            " jackin' ",
            Style::default()
                .bg(PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(PHOSPHOR_DARK)),
    ];
    spans.extend(loading_line_spans(view, frozen));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The `Loading <role> in <path>` line: one green colour throughout, the role
/// and the path **bold**, with a brightness ripple sweeping left→right so the
/// text reads as actively loading.
fn loading_line_spans(view: &LaunchView, frozen: bool) -> Vec<Span<'static>> {
    let Some(id) = view.identity.as_ref() else {
        return vec![Span::styled(
            "Preparing launch...",
            Style::default().fg(WHITE),
        )];
    };
    let prep = " in ";
    // Flatten to (char, bold) so the ripple can colour every glyph uniformly
    // while the role + path stay bold.
    let mut chars: Vec<(char, bool)> = Vec::new();
    for ch in "Loading ".chars() {
        chars.push((ch, false));
    }
    for ch in id.role.chars() {
        chars.push((ch, true));
    }
    for ch in prep.chars() {
        chars.push((ch, false));
    }
    for ch in id.target_label.chars() {
        chars.push((ch, true));
    }

    let len = chars.len();
    let lerp = |a: u8, b: u8, t: f32| (f32::from(b) - f32::from(a)).mul_add(t, f32::from(a)) as u8;
    // A bright band sweeps across the line every ~len+16 frames.
    let period = (len + 16) as f32;
    let peak = (view.frame as f32 % period) - 8.0;
    chars
        .into_iter()
        .enumerate()
        .map(|(i, (ch, bold))| {
            let bright = if frozen {
                0.0
            } else {
                (1.0 - (i as f32 - peak).abs() / 5.0).max(0.0)
            };
            // White, with the ripple brightening dim-white → full white.
            let v = lerp(150, 255, bright);
            let color = Color::Rgb(v, v, v);
            let mut style = Style::default().fg(color);
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

fn render_progress(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let lines = vec![blocks_line(view, frozen), labels_line(view, frozen)];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// One block per stage, filling gray (queued) -> green (done) so a glance
/// reads as a percent-complete bar; all green means loaded.
fn blocks_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let pulse = !frozen && (view.frame / STAGE_PULSE_PERIOD).is_multiple_of(2);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, row) in view.stages.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(BLOCK_GAP)));
        }
        // Thin horizontal segments (a slim progress bar), not tall full
        // blocks: heavy `━` for reached/active stages, light `─` for queued.
        let (glyph, color) = match row.status {
            StageStatus::Done => ('━', PHOSPHOR_GREEN),
            StageStatus::Running => ('━', if pulse { WHITE } else { PHOSPHOR_GREEN }),
            StageStatus::Skipped => ('━', PHOSPHOR_DIM),
            StageStatus::Failed => ('━', FAILED_RED),
            StageStatus::Blocked => ('━', WHITE),
            StageStatus::Queued => ('─', PHOSPHOR_DARK),
        };
        spans.push(Span::styled(
            glyph.to_string().repeat(BLOCK_WIDTH),
            Style::default().fg(color),
        ));
    }
    Line::from(spans)
}

fn labels_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let active = active_stage_index(view);
    let bright = !frozen && (view.frame / STAGE_PULSE_PERIOD).is_multiple_of(2);
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

/// The status-bar activity text: the current step with an upper-cased first
/// word and a trailing ellipsis (`wiring private network` -> `Wiring private
/// network…`). The live build/step detail lives only here, never inside the
/// box.
fn format_activity(status: &str) -> String {
    let trimmed = status
        .trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}…", first.to_uppercase(), chars.as_str())
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

fn render_footer(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, run_id: &str) {
    let instance = footer_instance(view);
    // The run id rides the status bar only in --debug, in amber, so the
    // operator is never unsure whether they are in a debug run; the blue
    // instance-id chip always shows once the container is named.
    let debug_chip = crate::tui::is_debug_mode().then_some(run_id);
    crate::console::widgets::status_bar::render(
        frame,
        area,
        &format_activity(&view.status),
        &instance,
        debug_chip,
    );
}

/// The container's short instance id once the container is named, else empty.
fn footer_instance(view: &LaunchView) -> String {
    view.identity
        .as_ref()
        .and_then(|identity| identity.container.as_deref())
        .and_then(jackin_protocol::instance_id_from_container_base)
        .map(str::to_string)
        .unwrap_or_default()
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

fn draw_select(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    title: &str,
    picker: &SelectListState,
) {
    let area = frame.area();
    render_launch_frame(frame, view, run_id, true, None);
    dim_buffer(frame, area);
    select_list::render(frame, picker_rect(area, picker), picker, title);
    render_picker_hints(frame, area);
}

/// Knock every cell behind the dialog back to a dim phosphor so the
/// modal reads as the foreground surface (matches the console modal-dim
/// rule). Runs after the frame is drawn and before the picker overlay.
fn dim_buffer(frame: &mut Frame<'_>, area: Rect) {
    let dark = Style::reset().fg(PHOSPHOR_DARK);
    let buf = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_style(dark);
        }
    }
}

fn picker_rect(area: Rect, picker: &SelectListState) -> Rect {
    // Interior: filter row + spacer + one row per item, plus two borders.
    let rows = u16::try_from(picker.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4);
    let height = rows.clamp(6, area.height.saturating_sub(2).max(6));
    let min_w = 40.min(area.width);
    let max_w = (area.width.saturating_mul(4) / 5).max(min_w);
    let width = picker.max_label_width().saturating_add(6).clamp(min_w, max_w);
    centered_rect(width, height, area)
}

fn render_picker_hints(frame: &mut Frame<'_>, area: Rect) {
    if area.height == 0 {
        return;
    }
    let row = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let key =
        |label| Span::styled(label, Style::default().fg(WHITE).add_modifier(Modifier::BOLD));
    let text = |label| Span::styled(label, Style::default().fg(PHOSPHOR_DIM));
    let line = Line::from(vec![
        key("↑/↓"),
        Span::raw(" "),
        text("navigate"),
        Span::raw("    "),
        text("type to filter"),
        Span::raw("    "),
        key("Enter"),
        Span::raw(" "),
        text("select"),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), row);
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
        };
        terminal
            .draw(|frame| render_launch_frame(frame, &view, "jk-run-42f9aa", true, None))
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Loading agent-smith in big-monorepo"));
        assert!(rendered.contains("construct"));
        // Footer chip shows the short instance id derived from the container.
        assert!(rendered.contains("k7p9m2xq"));

        view.failure = Some(LaunchFailure {
            title: "Docker unavailable".to_string(),
            summary: "docker daemon is not responding".to_string(),
            next_step: Some("Start Docker and run the command again.".to_string()),
            stage: LaunchStage::Network,
        });
        terminal
            .draw(|frame| render_launch_frame(frame, &view, "jk-run-42f9aa", true, None))
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Docker unavailable"));
        assert!(rendered.contains("docker daemon is not responding"));
        // The reused error_popup carries its own dismiss hint.
        assert!(rendered.contains("Enter/O"));
    }
}
