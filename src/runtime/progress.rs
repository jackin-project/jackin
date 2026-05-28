use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::console::widgets::error_popup::{self, ErrorPopupState};
use crate::console::widgets::select_list::{self, SelectListState};
use crate::console::widgets::{
    DANGER_RED, ModalOutcome, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE,
};
use crate::diagnostics::RunDiagnostics;
use jackin_tui::HintSpan;

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
    /// Operator dismissed the failure popup (Enter/Esc). The render task owns
    /// input, so it sets this flag; [`LaunchProgress::stage_failed`] awaits it
    /// rather than reading stdin itself (which would freeze the single-threaded
    /// executor and never let the render task draw the popup).
    failure_ack: bool,
    frame: usize,
    /// Operator opened the live docker-build log overlay (by clicking the
    /// footer activity). While open it hides the cockpit behind an opaque
    /// scrollable view of [`crate::runtime::build_log`].
    build_log_open: bool,
    /// Lines scrolled up from the tail of the build log (0 = follow the
    /// newest output).
    build_log_scroll: usize,
    /// Pointer is hovering the clickable footer activity (which opens the
    /// build-log overlay). Lifts the activity to the link colour.
    build_log_hover: bool,
    label_transition: Option<StageLabelTransition>,
}

#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub title: String,
    pub summary: String,
    pub next_step: Option<String>,
    pub stage: LaunchStage,
    pub diagnostics_path: Option<PathBuf>,
    pub command_output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
struct StageLabelTransition {
    from: usize,
    to: usize,
    start_frame: usize,
}

type SharedView = Arc<std::sync::Mutex<LaunchView>>;
const STAGE_VISUAL_SETTLE: Duration = Duration::from_millis(140);

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
                    // Drain input only while this task owns the renderer — when
                    // a forced-choice picker holds it, the picker reads events
                    // itself and this poll would steal its keystrokes.
                    handle_cockpit_input(&view);
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
        failure_ack: false,
        frame: 0,
        build_log_open: false,
        build_log_scroll: 0,
        build_log_hover: false,
        label_transition: None,
    }
}

impl LaunchProgress {
    pub fn new(diagnostics: Arc<RunDiagnostics>, no_motion: bool) -> anyhow::Result<Self> {
        let view: SharedView = Arc::new(std::sync::Mutex::new(initial_view()));
        let renderer = if rich_terminal_supported() {
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

    pub async fn stage_failed(&mut self, mut failure: LaunchFailure) {
        let stage = failure.stage;
        let summary = failure.summary.clone();
        let next_step = failure.next_step.clone();
        failure.diagnostics_path = Some(self.diagnostics.path().to_path_buf());
        if failure.command_output_path.is_none() {
            let docker_output = self.diagnostics.command_output_path("docker-build");
            if docker_output.exists() {
                failure.command_output_path = Some(docker_output);
            }
        }
        self.with_view(|v| {
            update_stage(v, stage, StageStatus::Failed, &summary);
            v.status.clone_from(&summary);
            v.failure_ack = false;
            v.failure = Some(failure);
        });
        self.diagnostics.stage(
            "stage_failed",
            stage.label(),
            &summary,
            next_step.as_deref(),
        );
        // On a rich surface the render task draws the failure popup and owns the
        // terminal's input; poll for the operator's Enter/Esc dismiss. Yielding
        // with an async sleep (rather than a blocking stdin read) is essential on
        // the single-threaded runtime — a blocking read would never let the
        // render task run, so the popup would neither draw nor receive the key.
        if matches!(self.renderer, Renderer::Rich(_)) {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let acked = self.view.lock().map_or(true, |v| v.failure_ack);
                if acked {
                    break;
                }
            }
        }
    }

    pub fn opening_hardline(&mut self) {
        self.stage_started(LaunchStage::Hardline, "opening hardline");
    }

    /// Give the rich renderer at least one visible frame after a stage change.
    ///
    /// Fast Docker/cache paths can otherwise advance from one stage to the next
    /// before the 33ms render tick observes the intermediate state, making the
    /// progress rail appear to skip labels. Compact/test renderers do not draw
    /// asynchronously, so they should not pay this delay.
    pub async fn settle_stage_visual(&self) {
        if matches!(self.renderer, Renderer::Rich(_)) {
            tokio::time::sleep(STAGE_VISUAL_SETTLE).await;
        }
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
            } else if detail.is_empty() {
                eprintln!("{label}: {}", status.label());
            } else {
                eprintln!("{label}: {} \u{2014} {detail}", status.label());
            }
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
    let previous_active = active_stage_index(view);
    if let Some(row) = view.stages.iter_mut().find(|row| row.stage == stage) {
        row.status = status;
        row.detail = detail.to_string();
    }
    let next_active = active_stage_index(view);
    if previous_active != next_active {
        view.label_transition = Some(StageLabelTransition {
            from: previous_active,
            to: next_active,
            start_frame: view.frame,
        });
    }
}

const BUILD_LOG_SCROLL_STEP: usize = 3;
const BUILD_LOG_PAGE_STEP: usize = 10;

/// Whether `(col, row)` falls on the footer activity text ("Building Docker
/// image…"). The footer is the last terminal row; the activity is left-aligned
/// and the right-side chips never overlap it, so a left-edge span is enough.
fn hit_activity(view: &LaunchView, col: u16, row: u16) -> bool {
    let Ok((_, rows)) = crossterm::terminal::size() else {
        return false;
    };
    if rows == 0 || row != rows - 1 {
        return false;
    }
    let width = u16::try_from(format_activity(&view.status).chars().count()).unwrap_or(u16::MAX);
    // One column of slack for the band's left padding.
    col <= width
}

/// Switch the terminal pointer to the hand/`pointer` shape over a clickable
/// element, or back to `default`, via OSC 22 — the same mechanism the
/// in-container multiplexer uses. Terminals without OSC 22 support ignore the
/// sequence harmlessly. Emitted between ratatui frames from the render task, so
/// it never interleaves with a frame write.
fn set_cockpit_pointer(pointer: bool) {
    use std::io::Write as _;
    let seq = if pointer {
        jackin_tui::ansi::POINTER_HAND
    } else {
        jackin_tui::ansi::POINTER_DEFAULT
    };
    let mut out = std::io::stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
}

/// Drain queued terminal input and fold it into the build-log overlay / failure
/// state.
///
/// Called only while the render task owns the renderer (no forced-choice picker
/// is reading events), so this poll cannot steal a picker's keystrokes. Polling
/// with a zero timeout keeps the 33 ms render cadence intact.
fn handle_cockpit_input(view: &SharedView) {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let Ok(ev) = event::read() else {
            return;
        };
        let Ok(mut v) = view.lock() else {
            return;
        };
        match ev {
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if v.build_log_open {
                        // The overlay covers the whole screen, so any click
                        // dismisses it back to the cockpit.
                        v.build_log_open = false;
                    } else if crate::runtime::build_log::len() > 0
                        && hit_activity(&v, m.column, m.row)
                    {
                        v.build_log_open = true;
                        v.build_log_scroll = 0;
                        // Overlay now covers the activity; clear its hover lift
                        // and drop the hand pointer.
                        v.build_log_hover = false;
                        set_cockpit_pointer(false);
                    }
                }
                MouseEventKind::Moved => {
                    // Hover the activity only while it is actually clickable
                    // (overlay closed and there is a build log to show). Lift
                    // its colour and switch to the hand pointer on enter; revert
                    // on leave — the same affordance the tabs use.
                    let hovering = !v.build_log_open
                        && crate::runtime::build_log::len() > 0
                        && hit_activity(&v, m.column, m.row);
                    if hovering != v.build_log_hover {
                        v.build_log_hover = hovering;
                        set_cockpit_pointer(hovering);
                    }
                }
                MouseEventKind::ScrollUp if v.build_log_open => {
                    v.build_log_scroll = v.build_log_scroll.saturating_add(BUILD_LOG_SCROLL_STEP);
                }
                MouseEventKind::ScrollDown if v.build_log_open => {
                    v.build_log_scroll = v.build_log_scroll.saturating_sub(BUILD_LOG_SCROLL_STEP);
                }
                _ => {}
            },
            Event::Key(k)
                if k.kind == KeyEventKind::Press
                    && v.failure.is_some()
                    && matches!(k.code, KeyCode::Enter | KeyCode::Esc) =>
            {
                // Failure popup is modal over the cockpit; Enter/Esc acknowledges
                // it so the awaiting `stage_failed` returns.
                v.failure_ack = true;
            }
            Event::Key(k) if k.kind == KeyEventKind::Press && v.build_log_open => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => v.build_log_open = false,
                KeyCode::Up => v.build_log_scroll = v.build_log_scroll.saturating_add(1),
                KeyCode::Down => v.build_log_scroll = v.build_log_scroll.saturating_sub(1),
                KeyCode::PageUp => {
                    v.build_log_scroll = v.build_log_scroll.saturating_add(BUILD_LOG_PAGE_STEP);
                }
                KeyCode::PageDown => {
                    v.build_log_scroll = v.build_log_scroll.saturating_sub(BUILD_LOG_PAGE_STEP);
                }
                _ => {}
            },
            _ => {}
        }
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
            crate::tui::set_rich_surface_active(false);
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
            crossterm::terminal::enable_raw_mode()
                .context("entering raw mode for launch picker")?;
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
            if let Event::Key(key) =
                crossterm::event::read().context("reading launch picker input")?
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
const LABEL_GAP: usize = 4;
const LABEL_SLIDE_FRAMES: usize = 12;

fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    no_motion: bool,
    rain: Option<&crate::tui::animation::RainState>,
) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    // The build-log overlay owns the whole screen behind an opaque backdrop,
    // matching the capsule modal convention (hide everything, don't dim).
    if view.build_log_open {
        render_build_log_dialog(frame, area, view);
        return;
    }

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
    // Write each cell straight into the frame buffer rather than building a
    // `Vec<Line<Span>>`: at 30fps a full field is width × height spans, each its
    // own `String`, every frame. RAIN_CHARS is ASCII (width-1), so one cell maps
    // to one buffer cell. An empty cell only sets its symbol so it keeps the
    // background already painted behind the rain.
    let buf = frame.buffer_mut();
    for y in 0..area.height {
        let grid_y = usize::from(area.y + y);
        let rows_from_bottom = area.height - 1 - y;
        let fade = if rows_from_bottom >= fade_rows {
            1.0
        } else {
            f32::from(rows_from_bottom) / f32::from(fade_rows)
        };
        let dim = |c: u8| (f32::from(c) * fade * fade_in) as u8;
        for x in 0..area.width {
            let grid_x = usize::from(area.x + x);
            let lit = rain
                .grid
                .get(grid_y)
                .and_then(|row| row.get(grid_x))
                .and_then(|cell| cell.as_ref())
                .and_then(|cell| {
                    crate::tui::animation::age_to_color(cell.age).map(|rgb| (cell.ch, rgb))
                });
            let cell = &mut buf[(area.x + x, area.y + y)];
            match lit {
                Some((ch, (r, g, b))) => {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(Color::Rgb(dim(r), dim(g), dim(b))));
                }
                None => {
                    cell.set_char(' ');
                }
            }
        }
    }
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
    // Flatten to (char, kind): 0 = normal ("Loading" / "in"), 1 = role,
    // 2 = path. The role renders white so it pops; the rest stays green. Role
    // and path are bold. The ripple brightens every glyph uniformly.
    let mut chars: Vec<(char, u8)> = Vec::new();
    for ch in "Loading ".chars() {
        chars.push((ch, 0));
    }
    for ch in id.role.chars() {
        chars.push((ch, 1));
    }
    for ch in prep.chars() {
        chars.push((ch, 0));
    }
    for ch in id.target_label.chars() {
        chars.push((ch, 2));
    }

    let len = chars.len();
    let lerp = |a: u8, b: u8, t: f32| (f32::from(b) - f32::from(a)).mul_add(t, f32::from(a)) as u8;
    // A bright band sweeps across the line every ~len+16 frames.
    let period = (len + 16) as f32;
    let peak = (view.frame as f32 % period) - 8.0;
    coalesce_cells(chars.into_iter().enumerate().map(|(i, (ch, kind))| {
        let bright = if frozen {
            0.0
        } else {
            (1.0 - (i as f32 - peak).abs() / 5.0).max(0.0)
        };
        let color = if kind == 0 {
            // "Loading" / "in": green, dim → bright on the ripple.
            Color::Rgb(
                lerp(0, 120, bright),
                lerp(140, 255, bright),
                lerp(30, 120, bright),
            )
        } else {
            // Role + path: white, brightening dim-white → full white.
            let v = lerp(170, 255, bright);
            Color::Rgb(v, v, v)
        };
        let mut style = Style::default().fg(color);
        if kind != 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        (ch, style)
    }))
}

/// Coalesce per-cell `(char, Style)` pairs into the fewest spans: consecutive
/// cells sharing a style merge into one span. Render paths that compute a style
/// per glyph (the loading-line ripple, the wrapped build log) would otherwise
/// allocate one `Span` plus one `String` per character every frame.
fn coalesce_cells(cells: impl IntoIterator<Item = (char, Style)>) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut cur: Option<Style> = None;
    for (ch, style) in cells {
        if cur != Some(style) {
            if let Some(prev) = cur.take() {
                spans.push(Span::styled(std::mem::take(&mut buf), prev));
            }
            cur = Some(style);
        }
        buf.push(ch);
    }
    if let Some(prev) = cur {
        spans.push(Span::styled(buf, prev));
    }
    spans
}

fn render_progress(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let lines = vec![
        blocks_line(view, frozen),
        labels_line(view, frozen, usize::from(area.width)),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn display_stage_statuses(view: &LaunchView) -> Vec<StageStatus> {
    if view.stages.is_empty() {
        return Vec::new();
    }

    let active = active_stage_index(view);
    view.stages
        .iter()
        .enumerate()
        .map(|(index, row)| {
            if index < active {
                if row.status == StageStatus::Failed {
                    StageStatus::Failed
                } else {
                    StageStatus::Done
                }
            } else if index == active {
                row.status
            } else {
                StageStatus::Queued
            }
        })
        .collect()
}

/// One block per stage, filling gray (queued) -> green (done) so a glance
/// reads as a percent-complete bar; all green means loaded.
fn blocks_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let pulse = !frozen && (view.frame / STAGE_PULSE_PERIOD).is_multiple_of(2);
    let display_statuses = display_stage_statuses(view);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, status) in display_statuses.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(BLOCK_GAP)));
        }
        // Thin horizontal segments (a slim progress bar), not tall full
        // blocks: heavy `━` for reached/active stages, light `─` for queued.
        let (glyph, color) = match status {
            StageStatus::Done => ('━', PHOSPHOR_GREEN),
            StageStatus::Running => ('━', if pulse { WHITE } else { PHOSPHOR_GREEN }),
            StageStatus::Skipped => ('━', PHOSPHOR_GREEN),
            StageStatus::Failed => ('━', DANGER_RED),
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

#[derive(Clone, Copy)]
struct LabelCell {
    ch: char,
    style: Style,
}

fn labels_line(view: &LaunchView, frozen: bool, width: usize) -> Line<'static> {
    if width == 0 || view.stages.is_empty() {
        return Line::from(String::new());
    }

    let active = active_stage_index(view);
    let bright = !frozen && (view.frame / STAGE_PULSE_PERIOD).is_multiple_of(2);
    let display_statuses = display_stage_statuses(view);
    let (strip, centers) = label_strip(view, active, bright, &display_statuses);
    let active_center = centers.get(active).copied().unwrap_or(0);
    let center = if frozen {
        active_center
    } else {
        animated_label_center(view, &centers).unwrap_or(active_center)
    };
    let start = center as isize - (width / 2) as isize;
    let cells = (0..width).map(|x| {
        let index = start + x as isize;
        if index >= 0 {
            strip
                .get(index as usize)
                .copied()
                .unwrap_or_else(blank_label_cell)
        } else {
            blank_label_cell()
        }
    });
    Line::from(coalesce_cells(cells.map(|cell| (cell.ch, cell.style))))
}

fn label_strip(
    view: &LaunchView,
    active: usize,
    bright: bool,
    display_statuses: &[StageStatus],
) -> (Vec<LabelCell>, Vec<usize>) {
    let mut cells = Vec::new();
    let mut centers = Vec::with_capacity(view.stages.len());
    for (index, row) in view.stages.iter().enumerate() {
        if index > 0 {
            cells.extend((0..LABEL_GAP).map(|_| blank_label_cell()));
        }
        let start = cells.len();
        let style = label_style_for_stage(
            display_statuses
                .get(index)
                .copied()
                .unwrap_or(StageStatus::Queued),
            index == active,
            bright,
        );
        let label = row.stage.label();
        cells.extend(label.chars().map(|ch| LabelCell { ch, style }));
        centers.push(start + label.chars().count() / 2);
    }
    (cells, centers)
}

fn label_style_for_stage(status: StageStatus, active: bool, bright: bool) -> Style {
    if active {
        return match status {
            StageStatus::Failed => Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
            _ if bright => Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            _ => Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        };
    }

    match status {
        StageStatus::Done | StageStatus::Skipped => Style::default().fg(PHOSPHOR_DIM),
        StageStatus::Failed => Style::default().fg(DANGER_RED),
        StageStatus::Running | StageStatus::Blocked => Style::default().fg(PHOSPHOR_GREEN),
        StageStatus::Queued => Style::default().fg(PHOSPHOR_DARK),
    }
}

fn blank_label_cell() -> LabelCell {
    LabelCell {
        ch: ' ',
        style: Style::default(),
    }
}

fn animated_label_center(view: &LaunchView, centers: &[usize]) -> Option<usize> {
    let transition = view.label_transition?;
    if transition.from == transition.to {
        return None;
    }
    let from = *centers.get(transition.from)?;
    let to = *centers.get(transition.to)?;
    let elapsed = view.frame.saturating_sub(transition.start_frame);
    if elapsed >= LABEL_SLIDE_FRAMES {
        return None;
    }
    let progress = elapsed as f32 / LABEL_SLIDE_FRAMES as f32;
    let eased = 1.0 - (1.0 - progress).powi(3);
    let center = (from as f32).mul_add(1.0 - eased, to as f32 * eased);
    Some(center.round() as usize)
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
    if let Some(failed) = view
        .stages
        .iter()
        .position(|row| row.status == StageStatus::Failed)
    {
        return failed;
    }

    let first_incomplete = view
        .stages
        .iter()
        .position(|row| !matches!(row.status, StageStatus::Done | StageStatus::Skipped));
    let Some(frontier) = first_incomplete else {
        return view.stages.len().saturating_sub(1);
    };
    if view.stages[frontier].status == StageStatus::Running {
        return frontier;
    }

    view.stages
        .iter()
        .position(|row| row.status == StageStatus::Running)
        .filter(|running| *running < frontier)
        .unwrap_or_else(|| frontier.saturating_sub(1))
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, run_id: &str) {
    let instance = footer_instance(view);
    // The run id rides the status bar only in --debug, in amber, so the
    // operator is never unsure whether they are in a debug run; the blue
    // instance-id chip always shows once the container is named.
    let debug_chip = crate::tui::is_debug_mode().then_some(run_id);
    // Fade the bar up from black over the first ~30 frames so it appears
    // gradually with the rain rather than popping in.
    #[allow(clippy::cast_precision_loss)]
    let alpha = (view.frame as f32 / 30.0).min(1.0);
    crate::console::widgets::status_bar::render(
        frame,
        area,
        &format_activity(&view.status),
        &instance,
        debug_chip,
        alpha,
        view.build_log_hover,
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
    let diagnostics = failure
        .diagnostics_path
        .as_ref()
        .map(|path| format!("\nrun diagnostics · {}", path.display()))
        .unwrap_or_default();
    let command_output = failure
        .command_output_path
        .as_ref()
        .map(|path| format!("\ndocker output · {}", path.display()))
        .unwrap_or_default();
    let message = format!(
        "{}\n\nstage · {}\nrun · {run_id}{diagnostics}{command_output}{next}",
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
    // The popup draws no hint of its own (footer-only-hints rule); show the
    // dismiss keys on the bottom row, over the now-frozen status bar.
    let hint_row = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    crate::console::widgets::hints::render(frame, hint_row, FAILURE_HINT);
}

/// Footer-hint keys for the launch failure popup (dismiss only).
const FAILURE_HINT: &[HintSpan<'static>] = &[HintSpan::Key("Enter/Esc"), HintSpan::Text("dismiss")];

/// Footer-hint keys for the forced-choice launch picker.
const PICKER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑/↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type to filter"),
    HintSpan::GroupSep,
    HintSpan::Key("Enter"),
    HintSpan::Text("select"),
];

/// Footer-hint keys for the build-log overlay. Shared `HintSpan` vocabulary,
/// rendered by the shared host hint renderer so it matches every other footer.
const BUILD_LOG_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("scroll"),
    HintSpan::GroupSep,
    HintSpan::Key("PgUp/PgDn"),
    HintSpan::Text("page"),
    HintSpan::GroupSep,
    HintSpan::Key("Esc"),
    HintSpan::Text("close"),
];

/// Full-screen opaque overlay over the live docker-build output, scrollable.
/// Opened by clicking the footer activity; dismissed by `Esc`/`q` or a click.
/// Long lines wrap inside the modal instead of requiring horizontal scroll;
/// continuation rows carry a visible prefix so wrapped Docker output remains
/// easy to distinguish from separate log lines. The key hint renders in the
/// bottom footer row, never inside the box (TUI design rule).
fn render_build_log_dialog(frame: &mut Frame<'_>, area: Rect, view: &LaunchView) {
    use crate::console::widgets::scrollable::{render_scrollable_block, viewport_height};

    // Opaque black backdrop fully hides the cockpit behind the overlay (same
    // solid look as the capsule modals).
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    // Bottom row is the footer hint; the bordered box takes the rest.
    let box_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let hint_area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };

    let title = if crate::runtime::build_log::is_active() {
        " Docker build · building… "
    } else {
        " Docker build "
    };
    // The full output drives the shared scrollable block so its proportional
    // scrollbar is correct. Cloning the (capped) buffer is acceptable here: the
    // overlay is a transient, operator-opened modal, not the steady cockpit.
    let raw = crate::runtime::build_log::snapshot();
    let viewport_w = crate::console::widgets::scrollable::viewport_width(box_area);
    let lines: Vec<Line<'_>> = if raw.is_empty() {
        vec![Line::from(Span::styled(
            "(waiting for docker build output…)",
            Style::default().fg(PHOSPHOR_DIM),
        ))]
    } else {
        wrap_build_log_lines(raw, viewport_w)
    };

    // `build_log_scroll` counts lines up from the tail (0 = follow newest).
    // Convert to the shared block's top-offset; render_scrollable_block clamps
    // and paints the green scrollbar only when the content overflows.
    let viewport_h = viewport_height(box_area);
    let max_top = lines.len().saturating_sub(viewport_h);
    let mut scroll_y =
        u16::try_from(max_top.saturating_sub(view.build_log_scroll)).unwrap_or(u16::MAX);
    let mut scroll_x = 0u16;
    render_scrollable_block(
        frame,
        box_area,
        lines,
        &mut scroll_x,
        &mut scroll_y,
        true,
        Some(title),
    );

    crate::console::widgets::hints::render(frame, hint_area, BUILD_LOG_HINT);
}

const BUILD_LOG_WRAP_PREFIX: &str = "↳ ";

fn wrap_build_log_lines(raw: Vec<String>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    raw.into_iter()
        .flat_map(|line| wrap_build_log_line(&line, width))
        .collect()
}

fn wrap_build_log_line(line: &str, width: usize) -> Vec<Line<'static>> {
    if line.is_empty() {
        return vec![Line::from(String::new())];
    }

    let default_style = Style::default().fg(Color::Gray).bg(Color::Black);
    let spans = crate::ansi_text::styled_spans(line.trim_end(), default_style);
    wrap_build_log_spans(spans, width)
}

fn wrap_build_log_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    let mut cells: Vec<(char, Style)> = Vec::new();
    for span in spans {
        let style = span.style;
        cells.extend(span.content.chars().map(|ch| (ch, style)));
    }
    if cells.is_empty() {
        return vec![Line::from(String::new())];
    }

    let mut lines = Vec::new();
    let continuation_width = width
        .saturating_sub(BUILD_LOG_WRAP_PREFIX.chars().count())
        .max(1);
    let mut pos = 0;
    let mut first_line = true;
    while pos < cells.len() {
        let limit = if first_line {
            width
        } else {
            continuation_width
        };
        let hard_end = pos.saturating_add(limit).min(cells.len());
        let (line_end, mut next) = if hard_end < cells.len()
            && let Some(space) = (pos + 1..hard_end)
                .rev()
                .find(|idx| cells[*idx].0.is_whitespace())
        {
            (space, space + 1)
        } else {
            (hard_end, hard_end)
        };
        while next < cells.len() && cells[next].0.is_whitespace() {
            next += 1;
        }
        let line_cells = if line_end == pos {
            &cells[pos..hard_end]
        } else {
            &cells[pos..line_end]
        };
        push_wrapped_build_line(&mut lines, spans_from_cells(line_cells), first_line);
        first_line = false;
        pos = if line_end == pos { hard_end } else { next };
    }
    lines
}

fn spans_from_cells(cells: &[(char, Style)]) -> Vec<Span<'static>> {
    coalesce_cells(cells.iter().copied())
}

fn push_wrapped_build_line(
    lines: &mut Vec<Line<'static>>,
    mut spans: Vec<Span<'static>>,
    first_line: bool,
) {
    if !first_line {
        spans.insert(
            0,
            Span::styled(
                BUILD_LOG_WRAP_PREFIX,
                Style::default().fg(PHOSPHOR_DIM).bg(Color::Black),
            ),
        );
    }
    lines.push(Line::from(spans));
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
    let width = picker
        .max_label_width()
        .saturating_add(6)
        .clamp(min_w, max_w);
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
    crate::console::widgets::hints::render(frame, row, PICKER_HINT);
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

    fn test_diagnostics() -> std::sync::Arc<RunDiagnostics> {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        RunDiagnostics::start(&paths, false, "load").unwrap()
    }

    fn dummy_failure() -> LaunchFailure {
        LaunchFailure {
            title: "boom".to_string(),
            summary: "it failed".to_string(),
            next_step: None,
            stage: LaunchStage::Network,
            diagnostics_path: None,
            command_output_path: None,
        }
    }

    #[tokio::test]
    async fn stage_failed_does_not_block_on_non_rich_renderer() {
        // The Rich path waits for an operator Enter/Esc dismiss; the Test/Compact
        // path must return immediately, or a non-TTY / CI launch would hang
        // forever on the first failure.
        let mut progress = LaunchProgress::for_test(test_diagnostics());
        tokio::time::timeout(
            std::time::Duration::from_millis(500),
            progress.stage_failed(dummy_failure()),
        )
        .await
        .expect("stage_failed must not block on a non-rich renderer");
        assert!(progress.view.lock().unwrap().failure.is_some());
        assert!(!progress.view.lock().unwrap().failure_ack);
    }

    #[tokio::test]
    async fn stage_failed_resets_prior_ack() {
        // A second failure must start un-acked: a stale ack left over from a
        // previously dismissed popup would otherwise auto-dismiss the new one.
        let mut progress = LaunchProgress::for_test(test_diagnostics());
        progress.stage_failed(dummy_failure()).await;
        progress.view.lock().unwrap().failure_ack = true;
        progress.stage_failed(dummy_failure()).await;
        assert!(!progress.view.lock().unwrap().failure_ack);
    }

    #[test]
    fn select_choice_returns_none_on_non_rich_renderer() {
        // No picker without a rich surface; the caller falls back to the plain
        // stdin prompt, so this must be Ok(None) — not Err, not a default index.
        let mut progress = LaunchProgress::for_test(test_diagnostics());
        let choice = progress
            .select_choice("pick", vec!["a".into(), "b".into()])
            .unwrap();
        assert!(choice.is_none());
    }

    #[test]
    fn update_stage_sets_one_rows_status_and_detail() {
        let mut view = initial_view();
        update_stage(&mut view, LaunchStage::Network, StageStatus::Done, "up");
        let net = view
            .stages
            .iter()
            .find(|r| r.stage == LaunchStage::Network)
            .unwrap();
        assert_eq!(net.status, StageStatus::Done);
        assert_eq!(net.detail, "up");
        // A different stage is left untouched.
        let cap = view
            .stages
            .iter()
            .find(|r| r.stage == LaunchStage::Capsule)
            .unwrap();
        assert_ne!(cap.status, StageStatus::Done);
    }

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

    #[tokio::test]
    async fn test_renderer_does_not_delay_stage_settle() {
        let progress = LaunchProgress::for_test(test_diagnostics());
        tokio::time::timeout(Duration::from_millis(20), progress.settle_stage_visual())
            .await
            .expect("test renderer should not sleep");
    }

    #[test]
    fn failed_stage_is_the_active_progress_label() {
        let mut view = initial_view();
        update_stage(
            &mut view,
            LaunchStage::Credentials,
            StageStatus::Done,
            "ready",
        );
        update_stage(
            &mut view,
            LaunchStage::Construct,
            StageStatus::Done,
            "ready",
        );
        update_stage(
            &mut view,
            LaunchStage::DerivedImage,
            StageStatus::Failed,
            "Building the Docker container failed.",
        );

        assert_eq!(
            view.stages[active_stage_index(&view)].stage,
            LaunchStage::DerivedImage
        );
        let labels = labels_line(&view, true, 80);
        let failed = labels
            .spans
            .iter()
            .find(|span| span.content == "derived image")
            .expect("failed stage label should be visible");
        assert_eq!(failed.style.fg, Some(DANGER_RED));
    }

    #[test]
    fn progress_display_masks_out_of_order_completed_stages() {
        let mut view = initial_view();
        update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
        update_stage(
            &mut view,
            LaunchStage::Role,
            StageStatus::Running,
            "resolving role",
        );
        update_stage(
            &mut view,
            LaunchStage::Workspace,
            StageStatus::Done,
            "materialized early",
        );

        let statuses = display_stage_statuses(&view);
        assert_eq!(statuses[0], StageStatus::Done);
        assert_eq!(statuses[1], StageStatus::Running);
        assert!(
            statuses[2..]
                .iter()
                .all(|status| *status == StageStatus::Queued),
            "later out-of-order completions must not punch green holes in the progress rail: {statuses:?}"
        );
    }

    #[test]
    fn progress_display_fills_every_prior_stage_sequentially() {
        let mut view = initial_view();
        update_stage(
            &mut view,
            LaunchStage::Identity,
            StageStatus::Skipped,
            "already known",
        );
        update_stage(&mut view, LaunchStage::Role, StageStatus::Done, "trusted");
        update_stage(
            &mut view,
            LaunchStage::Credentials,
            StageStatus::Done,
            "resolved",
        );
        update_stage(
            &mut view,
            LaunchStage::Construct,
            StageStatus::Done,
            "online",
        );
        update_stage(
            &mut view,
            LaunchStage::DerivedImage,
            StageStatus::Running,
            "building",
        );

        let statuses = display_stage_statuses(&view);
        assert_eq!(
            &statuses[..5],
            &[
                StageStatus::Done,
                StageStatus::Done,
                StageStatus::Done,
                StageStatus::Done,
                StageStatus::Running,
            ]
        );
    }

    #[test]
    fn active_stage_uses_the_sequential_frontier() {
        let mut view = initial_view();
        update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
        update_stage(
            &mut view,
            LaunchStage::Workspace,
            StageStatus::Running,
            "polling workspace",
        );

        assert_eq!(
            view.stages[active_stage_index(&view)].stage,
            LaunchStage::Identity
        );
    }

    #[test]
    fn stage_label_transition_slides_between_centers() {
        let mut view = initial_view();
        update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
        update_stage(
            &mut view,
            LaunchStage::Role,
            StageStatus::Running,
            "resolving role",
        );

        let transition = view
            .label_transition
            .expect("active stage change should start a label slide");
        assert_eq!(transition.from, 0);
        assert_eq!(transition.to, 1);

        view.frame = transition.start_frame + LABEL_SLIDE_FRAMES / 2;
        let active = active_stage_index(&view);
        let display_statuses = display_stage_statuses(&view);
        let (_, centers) = label_strip(&view, active, false, &display_statuses);
        let center = animated_label_center(&view, &centers).unwrap();
        assert!(center > centers[0], "label viewport should move right");
        assert!(
            center < centers[1],
            "label viewport should not snap to the target"
        );
    }

    #[test]
    fn build_log_lines_wrap_with_visible_continuation() {
        let lines = wrap_build_log_lines(
            vec![
                "#5 RUN current_gid=\"$(id -g agent)\" && \x1b[31mcurrent_uid=\"$(id -u agent)\"\x1b[0m"
                    .to_string(),
            ],
            32,
        );

        assert!(lines.len() > 1);
        assert!(crate::console::widgets::scrollable::max_line_width(&lines) <= 32);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| &*span.content)
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(rendered[0], "#5 RUN current_gid=\"$(id -g");
        assert!(
            rendered[1].starts_with(BUILD_LOG_WRAP_PREFIX),
            "continuation row must be visually marked: {rendered:?}"
        );
        assert!(
            lines
                .iter()
                .flat_map(|line| &line.spans)
                .any(|span| span.style.fg == Some(Color::Red)),
            "ANSI foreground color should survive in the on-screen build log"
        );
        assert!(
            lines
                .iter()
                .flat_map(|line| &line.spans)
                .all(|span| !span.content.contains('\x1b')),
            "ANSI escape bytes should be interpreted, not rendered literally"
        );
    }

    #[test]
    fn build_log_dialog_wraps_long_lines_without_horizontal_scrollbar() {
        crate::runtime::build_log::begin();
        crate::runtime::build_log::push_line(
            "#4 FROM docker.io/projectjackin/jackin-the-architect:latest@sha256:08d62f4027f941d8f5ee1742b6b0ba9e8a3e276ab7626967b0e1de27917a0e94",
        );
        crate::runtime::build_log::end();

        let backend = TestBackend::new(56, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let view = LaunchView {
            identity: None,
            stages: Vec::new(),
            status: String::new(),
            failure: None,
            failure_ack: false,
            frame: 0,
            build_log_open: true,
            build_log_scroll: 0,
            build_log_hover: false,
            label_transition: None,
        };
        terminal
            .draw(|frame| render_build_log_dialog(frame, frame.area(), &view))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = format!("{buffer:?}");
        assert!(rendered.contains(BUILD_LOG_WRAP_PREFIX));
        let bottom = 10;
        let horizontal_scroll_cells = (1..55)
            .filter(|x| ["━", "·"].contains(&buffer[(*x, bottom)].symbol()))
            .count();
        assert_eq!(
            horizontal_scroll_cells, 0,
            "wrapped lines should fit the viewport and avoid horizontal scrollbar"
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
            failure_ack: false,
            frame: 0,
            build_log_open: false,
            build_log_scroll: 0,
            build_log_hover: false,
            label_transition: None,
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
            diagnostics_path: None,
            command_output_path: None,
        });
        terminal
            .draw(|frame| render_launch_frame(frame, &view, "jk-run-42f9aa", true, None))
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Docker unavailable"));
        assert!(rendered.contains("docker daemon is not responding"));
        // The dismiss hint shows in the footer (the popup draws none itself).
        assert!(rendered.contains("dismiss"));
    }
}
