use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use jackin_tui::components::{
    ConfirmState, ErrorPopupState, SelectListState, TextInputState, confirm_required_height,
    confirm_width_pct, render_confirm_dialog, render_error_dialog, render_hint_bar,
    render_scrollable_block, render_select_list, render_status_footer, render_text_input,
    required_height as error_dialog_required_height, viewport_height, viewport_width,
};
use jackin_tui::runtime::{NoEffect, UpdateResult};
use jackin_tui::theme::{
    DANGER_RED, DIALOG_BACKDROP, DIALOG_SURFACE, LINK_BLUE, PHOSPHOR_DARK, PHOSPHOR_DIM,
    PHOSPHOR_GREEN, WHITE,
};
use jackin_tui::{HintSpan, ModalOutcome};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
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
    AgentBinaries,
    DerivedImage,
    Workspace,
    Network,
    Sidecar,
    Capsule,
    Hardline,
}

impl LaunchStage {
    pub const ALL: [Self; 11] = [
        Self::Identity,
        Self::Role,
        Self::Credentials,
        Self::Construct,
        Self::AgentBinaries,
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
            Self::AgentBinaries => "agent binaries",
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
    build_log_scroll: jackin_tui::scroll::TailScroll,
    /// Pointer is hovering the clickable footer activity (which opens the
    /// build-log overlay). Lifts the activity to the link colour.
    build_log_hover: bool,
    label_transition: Option<StageLabelTransition>,
    /// Pointer is hovering a copyable value in the failure popup.
    failure_copy_hover: Option<FailureCopyTarget>,
    /// Last failure-popup value copied via OSC 52. Drives visible feedback.
    failure_copied: Option<FailureCopyTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureCopyTarget {
    RunId,
    DiagnosticsPath,
    CommandOutputPath,
}

#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub title: String,
    pub summary: String,
    pub detail: Option<String>,
    pub next_step: Option<String>,
    pub stage: LaunchStage,
    pub diagnostics_path: Option<PathBuf>,
    pub command_output_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum LaunchMessage {
    Started(LaunchIdentity),
    IdentityUpdated(LaunchIdentity),
    StageStatus {
        stage: LaunchStage,
        status: StageStatus,
        detail: String,
        set_activity: bool,
    },
    StageFailed(LaunchFailure),
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
                    handle_cockpit_input(&view, &run_id);
                    let snapshot = match view.lock() {
                        Ok(mut v) => {
                            if !rr.no_motion {
                                v.frame = v.frame.wrapping_add(1);
                            }
                            if v.build_log_open {
                                let area = crossterm::terminal::size()
                                    .ok()
                                    .map(|(width, height)| Rect::new(0, 0, width, height))
                                    .unwrap_or_default();
                                let filled = build_log_scroll_filled(area);
                                v.build_log_scroll.clamp(filled);
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
        build_log_scroll: jackin_tui::scroll::TailScroll::default(),
        build_log_hover: false,
        label_transition: None,
        failure_copy_hover: None,
        failure_copied: None,
    }
}

type LaunchUpdate = UpdateResult<NoEffect>;

fn update_launch_view(view: &mut LaunchView, msg: LaunchMessage) -> LaunchUpdate {
    match msg {
        LaunchMessage::Started(identity) => {
            let preposition = identity.target_kind.launch_preposition();
            view.status = format!("loading {} {preposition}", identity.role);
            view.identity = Some(identity);
        }
        LaunchMessage::IdentityUpdated(identity) => {
            view.identity = Some(identity);
        }
        LaunchMessage::StageStatus {
            stage,
            status,
            detail,
            set_activity,
        } => {
            update_stage(view, stage, status, &detail);
            if set_activity {
                view.status = detail;
            }
        }
        LaunchMessage::StageFailed(failure) => {
            let stage = failure.stage;
            let summary = failure.summary.clone();
            update_stage(view, stage, StageStatus::Failed, &summary);
            view.status = summary;
            view.failure_ack = false;
            view.failure_copy_hover = None;
            view.failure_copied = None;
            view.failure = Some(failure);
        }
    }
    UpdateResult::redraw()
}

impl LaunchProgress {
    pub fn new(diagnostics: Arc<RunDiagnostics>, no_motion: bool) -> anyhow::Result<Self> {
        require_rich_terminal()?;
        let view: SharedView = Arc::new(std::sync::Mutex::new(initial_view()));
        let rich = RichRenderer::enter(no_motion)?;
        let renderer = Renderer::Rich(RichDriver::spawn(
            rich,
            view.clone(),
            diagnostics.run_id().to_string(),
        ));
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

    fn update_view(&self, msg: LaunchMessage) {
        self.with_view(|view| {
            let _dirty = update_launch_view(view, msg);
        });
    }

    pub fn started(&mut self, identity: LaunchIdentity) {
        self.update_view(LaunchMessage::Started(identity));
        self.diagnostics.compact(
            "launch_started",
            &format!("diagnostics: run {}", self.run_id()),
        );
    }

    pub fn update_identity(&mut self, identity: LaunchIdentity) {
        self.update_view(LaunchMessage::IdentityUpdated(identity));
    }

    pub fn stage_started(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_view(LaunchMessage::StageStatus {
            stage,
            status: StageStatus::Running,
            detail: detail.clone(),
            set_activity: true,
        });
        self.diagnostics
            .stage("stage_started", stage.label(), &detail, None);
    }

    pub fn stage_progress(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_view(LaunchMessage::StageStatus {
            stage,
            status: StageStatus::Running,
            detail: detail.clone(),
            set_activity: true,
        });
        self.diagnostics
            .stage("stage_progress", stage.label(), &detail, None);
    }

    pub fn stage_done(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        let detail = detail.into();
        self.update_view(LaunchMessage::StageStatus {
            stage,
            status: StageStatus::Done,
            detail: detail.clone(),
            set_activity: false,
        });
        self.diagnostics
            .stage("stage_done", stage.label(), &detail, None);
    }

    pub fn stage_skipped(&mut self, stage: LaunchStage, reason: impl Into<String>) {
        let reason = reason.into();
        self.update_view(LaunchMessage::StageStatus {
            stage,
            status: StageStatus::Skipped,
            detail: reason.clone(),
            set_activity: false,
        });
        self.diagnostics
            .stage("stage_skipped", stage.label(), &reason, None);
    }

    pub async fn stage_failed(&mut self, mut failure: LaunchFailure) {
        let stage = failure.stage;
        let summary = failure.summary.clone();
        let next_step = failure.next_step.clone();
        let detail = failure.detail.clone();
        failure.diagnostics_path = Some(self.diagnostics.path().to_path_buf());
        if failure.command_output_path.is_none() {
            let docker_output = self.diagnostics.command_output_path("docker-build");
            if docker_output.exists() {
                failure.command_output_path = Some(docker_output);
            }
        }
        self.update_view(LaunchMessage::StageFailed(failure));
        self.diagnostics.stage(
            "stage_failed",
            stage.label(),
            &summary,
            detail.as_deref().or(next_step.as_deref()),
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
    /// progress rail appear to skip labels. Test renderers do not draw
    /// asynchronously, so they should not pay this delay.
    pub async fn settle_stage_visual(&self) {
        if matches!(self.renderer, Renderer::Rich(_)) {
            tokio::time::sleep(STAGE_VISUAL_SETTLE).await;
        }
    }

    /// Stop the render task and release the rich surface before the interactive
    /// handoff, so the capsule attach owns the terminal alone. Idempotent;
    /// no-op for the test renderer.
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

    /// Reclaim the rich renderer from the background render task and run a
    /// modal dialog against it. The task try-locks per frame, so it simply
    /// skips frames while the modal holds the lock. Bails when the launch is
    /// not driving the rich surface — `what` names the dialog in that error.
    fn with_rich_renderer<T>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut RichRenderer) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        if let Renderer::Rich(driver) = &mut self.renderer {
            let mut renderer = driver
                .renderer
                .lock()
                .map_err(|_| anyhow::anyhow!("launch renderer mutex poisoned"))?;
            f(&mut renderer)
        } else {
            anyhow::bail!("{what} requires the rich launch dialog")
        }
    }

    /// Present a forced-choice picker over `items` and return the chosen
    /// index. The picker cannot be cancelled — the operator must commit one
    /// of the options.
    pub fn select_choice(&mut self, title: &str, items: Vec<String>) -> anyhow::Result<usize> {
        self.with_rich_renderer("launch choice", |renderer| renderer.select(title, items))
    }

    pub fn prompt_text(
        &mut self,
        title: &str,
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        self.with_rich_renderer("manifest env text prompt", |renderer| {
            renderer.prompt_text(title, default.unwrap_or_default(), skippable)
        })
    }

    pub fn prompt_select(
        &mut self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        self.with_rich_renderer("manifest env select prompt", |renderer| {
            renderer.prompt_select(title, options, default, skippable)
        })
    }

    pub fn confirm_prompt(&mut self, prompt: impl Into<String>) -> anyhow::Result<bool> {
        self.with_rich_renderer("launch confirmation", |renderer| {
            renderer.confirm(ConfirmState::new(prompt))
        })
    }

    pub fn confirm_role_trust(
        &mut self,
        role: impl Into<String>,
        repository: impl Into<String>,
    ) -> anyhow::Result<bool> {
        self.with_rich_renderer("role trust prompt", |renderer| {
            renderer.confirm(ConfirmState::role_trust(role, repository))
        })
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

pub fn prelaunch_select_choice(
    no_motion: bool,
    title: &str,
    items: Vec<String>,
) -> anyhow::Result<usize> {
    require_rich_terminal()?;
    let mut renderer = RichRenderer::enter(no_motion)?;
    renderer.select(title, items)
}

/// Standalone forced-choice picker with a `context` block above the options.
///
/// For callers that run after the launch progress surface has been torn down
/// — the post-attach worktree-cleanup prompt. Enters its own rich surface (or
/// draws into the host guard's screen when one is active).
pub fn standalone_select_with_context(
    title: &str,
    context: &[Line<'_>],
    items: Vec<String>,
) -> anyhow::Result<usize> {
    let mut renderer = RichRenderer::enter_dialog(false)?;
    renderer.select_with_context(title, context, items)
}

/// Standalone error popup for launch-adjacent failures that need operator
/// acknowledgement in the same rich surface.
pub fn standalone_error_popup(title: &str, message: &str) -> anyhow::Result<()> {
    let mut renderer = RichRenderer::enter_dialog(false)?;
    renderer.error_popup(title, message)
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

fn build_log_scroll_filled(area: Rect) -> usize {
    let box_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let viewport_w = viewport_width(box_area);
    let viewport_h = viewport_height(box_area);
    let raw = crate::runtime::build_log::snapshot();
    let line_count = if raw.is_empty() {
        1
    } else {
        wrap_build_log_lines(raw, viewport_w).len()
    };
    jackin_tui::scroll::max_offset(line_count, viewport_h)
}

fn scroll_build_log(view: &mut LaunchView, area: Rect, delta: isize) {
    let filled = build_log_scroll_filled(area);
    view.build_log_scroll.scroll_by(filled, delta);
}

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
fn handle_cockpit_input(view: &SharedView, run_id: &str) {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
    let area = crossterm::terminal::size()
        .ok()
        .map(|(width, height)| Rect::new(0, 0, width, height))
        .unwrap_or_default();
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
                    if let Some(failure) = v.failure.as_ref() {
                        if let Some(target) =
                            failure_copy_target_at(area, failure, run_id, m.column, m.row)
                            && let Some(payload) = failure_copy_payload(failure, run_id, target)
                        {
                            let mut out = std::io::stdout();
                            let copy_ok = out
                                .write_all(&jackin_tui::ansi::encode_osc52_clipboard_write(
                                    &payload,
                                ))
                                .and_then(|()| out.flush())
                                .is_ok();
                            if copy_ok {
                                v.failure_copied = Some(target);
                            } else {
                                // Stdout detached or the terminal does not parse OSC 52
                                // (e.g. VTE on the BEL form). Don't flash a "Copied!"
                                // badge when nothing actually reached the clipboard;
                                // land a breadcrumb in the diagnostics run for triage.
                                crate::tui::emit_compact_line(
                                    "failure-popup-copy",
                                    "OSC 52 clipboard write failed — badge suppressed",
                                );
                            }
                        }
                    } else if v.build_log_open {
                        // The overlay covers the whole screen, so any click
                        // dismisses it back to the cockpit.
                        v.build_log_open = false;
                    } else if crate::runtime::build_log::len() > 0
                        && hit_activity(&v, m.column, m.row)
                    {
                        v.build_log_open = true;
                        v.build_log_scroll = jackin_tui::scroll::TailScroll::default();
                        // Overlay now covers the activity; clear its hover lift
                        // and drop the hand pointer.
                        v.build_log_hover = false;
                        set_cockpit_pointer(false);
                    }
                }
                MouseEventKind::Moved => {
                    if let Some(failure) = v.failure.as_ref() {
                        let hover = failure_copy_target_at(area, failure, run_id, m.column, m.row);
                        if hover != v.failure_copy_hover {
                            v.failure_copy_hover = hover;
                            set_cockpit_pointer(hover.is_some());
                        }
                        continue;
                    }
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
                    scroll_build_log(&mut v, area, BUILD_LOG_SCROLL_STEP as isize);
                }
                MouseEventKind::ScrollDown if v.build_log_open => {
                    scroll_build_log(&mut v, area, -(BUILD_LOG_SCROLL_STEP as isize));
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
                v.failure_copy_hover = None;
                set_cockpit_pointer(false);
            }
            Event::Key(k) if k.kind == KeyEventKind::Press && v.build_log_open => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => v.build_log_open = false,
                KeyCode::Up => scroll_build_log(&mut v, area, 1),
                KeyCode::Down => scroll_build_log(&mut v, area, -1),
                KeyCode::PageUp => {
                    scroll_build_log(&mut v, area, BUILD_LOG_PAGE_STEP as isize);
                }
                KeyCode::PageDown => {
                    scroll_build_log(&mut v, area, -(BUILD_LOG_PAGE_STEP as isize));
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
    }
}

struct RichRenderer {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    no_motion: bool,
    /// Whether this renderer entered the alternate screen on construction.
    /// Recorded so `drop` can leave it only when we entered it — under the
    /// host `TerminalSession` guard the screen persists into the capsule attach.
    entered_alt_screen: bool,
    /// Shared digital-rain engine (the same one the intro/outro use), ticked
    /// per frame and painted into the loading box. Sized to the terminal so
    /// the box shows a window into one continuous rainfall.
    rain: Option<crate::tui::animation::RainState>,
}

impl RichRenderer {
    fn enter_with_check(
        no_motion: bool,
        terminal_check: impl FnOnce() -> anyhow::Result<()>,
    ) -> anyhow::Result<Self> {
        terminal_check()?;
        let mut stdout = std::io::stdout();
        // When the launch flow's host guard already owns the alternate screen,
        // draw into it; only enter it ourselves when running standalone.
        let entered_alt_screen = !crate::tui::host_screen_owned();
        if entered_alt_screen {
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
            entered_alt_screen,
            rain: None,
        })
    }

    fn enter(no_motion: bool) -> anyhow::Result<Self> {
        Self::enter_with_check(no_motion, require_rich_terminal)
    }

    fn enter_dialog(no_motion: bool) -> anyhow::Result<Self> {
        Self::enter_with_check(no_motion, || Ok(()))
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

    /// Run a modal dialog loop with raw mode held for its duration so key
    /// events arrive un-buffered, restoring it on every exit path. The host
    /// guard already holds raw mode for the whole flow; only toggle it when
    /// this renderer is running standalone. `Ctrl-C` aborts the launch.
    fn with_raw_mode<T>(
        &mut self,
        context: &'static str,
        f: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let owns_raw = self.entered_alt_screen;
        if owns_raw {
            crossterm::terminal::enable_raw_mode().context(context)?;
        }
        let outcome = f(self);
        if owns_raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        outcome
    }

    /// Present a forced-choice picker over the dimmed launch frame.
    fn select(&mut self, title: &str, items: Vec<String>) -> anyhow::Result<usize> {
        self.with_raw_mode("entering raw mode for launch picker", |renderer| {
            renderer.select_loop(title, &[], items)
        })
    }

    /// Forced-choice picker with a descriptive `context` block above the
    /// options. Used by the standalone post-attach cleanup prompt.
    fn select_with_context(
        &mut self,
        title: &str,
        context: &[Line<'_>],
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        self.with_raw_mode("entering raw mode for cleanup picker", |renderer| {
            renderer.select_loop(title, context, items)
        })
    }

    fn error_popup(&mut self, title: &str, message: &str) -> anyhow::Result<()> {
        self.with_raw_mode("entering raw mode for error popup", |renderer| {
            renderer.error_popup_loop(title, message)
        })
    }

    fn select_loop(
        &mut self,
        title: &str,
        context: &[Line<'_>],
        items: Vec<String>,
    ) -> anyhow::Result<usize> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
        let mut picker = SelectListState::new(items);
        loop {
            self.terminal
                .draw(|frame| draw_select(frame, title, context, &picker))
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

    fn prompt_text(
        &mut self,
        title: &str,
        initial: &str,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        self.with_raw_mode("entering raw mode for launch env prompt", |renderer| {
            renderer.prompt_text_loop(title, initial, skippable)
        })
    }

    fn prompt_text_loop(
        &mut self,
        title: &str,
        initial: &str,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
        let mut input = if skippable {
            TextInputState::new_allow_empty(title, initial)
        } else {
            TextInputState::new(title, initial)
        };
        loop {
            self.terminal
                .draw(|frame| draw_text_prompt(frame, &input, skippable))
                .context("rendering launch env text prompt")?;
            if let Event::Key(key) =
                crossterm::event::read().context("reading launch env prompt input")?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    anyhow::bail!("launch cancelled by operator");
                }
                match input.handle_key(key) {
                    ModalOutcome::Commit(value) if value.is_empty() && skippable => {
                        return Ok(crate::env_resolver::PromptResult::Skipped);
                    }
                    ModalOutcome::Commit(value) => {
                        return Ok(crate::env_resolver::PromptResult::Value(value));
                    }
                    ModalOutcome::Cancel => anyhow::bail!("launch cancelled by operator"),
                    ModalOutcome::Continue => {}
                }
            }
        }
    }

    fn prompt_select(
        &mut self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        self.with_raw_mode("entering raw mode for launch env select", |renderer| {
            renderer.prompt_select_loop(title, options, default, skippable)
        })
    }

    fn prompt_select_loop(
        &mut self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<crate::env_resolver::PromptResult> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
        let mut items = options.to_vec();
        if skippable {
            items.push("(skip)".to_string());
        }
        let mut picker = SelectListState::new(items);
        if let Some(default) = default
            && let Some(index) = options.iter().position(|option| option == default)
        {
            picker.select_index(index);
        }
        loop {
            self.terminal
                .draw(|frame| draw_select(frame, title, &[], &picker))
                .context("rendering launch env select prompt")?;
            if let Event::Key(key) =
                crossterm::event::read().context("reading launch env select input")?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    anyhow::bail!("launch cancelled by operator");
                }
                match picker.handle_key(key) {
                    ModalOutcome::Commit(index) if skippable && index == options.len() => {
                        return Ok(crate::env_resolver::PromptResult::Skipped);
                    }
                    ModalOutcome::Commit(index) => {
                        return Ok(crate::env_resolver::PromptResult::Value(
                            options[index].clone(),
                        ));
                    }
                    ModalOutcome::Cancel => anyhow::bail!("launch cancelled by operator"),
                    ModalOutcome::Continue => {}
                }
            }
        }
    }

    fn confirm(&mut self, mut state: ConfirmState) -> anyhow::Result<bool> {
        self.with_raw_mode("entering raw mode for launch confirmation", |renderer| {
            renderer.confirm_loop(&mut state)
        })
    }

    fn confirm_loop(&mut self, state: &mut ConfirmState) -> anyhow::Result<bool> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
        loop {
            self.terminal
                .draw(|frame| draw_confirm(frame, state))
                .context("rendering launch confirmation")?;
            if let Event::Key(key) =
                crossterm::event::read().context("reading launch confirmation input")?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    anyhow::bail!("launch cancelled by operator");
                }
                match state.handle_key(key) {
                    ModalOutcome::Commit(confirmed) => return Ok(confirmed),
                    ModalOutcome::Cancel => return Ok(false),
                    ModalOutcome::Continue => {}
                }
            }
        }
    }

    fn error_popup_loop(&mut self, title: &str, message: &str) -> anyhow::Result<()> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
        let state = ErrorPopupState::new(title, message);
        loop {
            self.terminal
                .draw(|frame| draw_error_popup(frame, &state))
                .context("rendering launch error popup")?;
            if let Event::Key(key) =
                crossterm::event::read().context("reading error popup input")?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    anyhow::bail!("launch cancelled by operator");
                }
                match state.handle_key(key) {
                    ModalOutcome::Cancel => return Ok(()),
                    ModalOutcome::Continue => {}
                    ModalOutcome::Commit(()) => unreachable!("error popup never commits"),
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
        if self.entered_alt_screen {
            let _ = self.terminal.backend_mut().execute(LeaveAlternateScreen);
        }
        let _ = std::io::stdout().flush();
    }
}

pub(crate) fn rich_terminal_supported() -> bool {
    terminal_supports_rich_surface(true)
}

/// Bail with the canonical rich-terminal requirement message unless the
/// current terminal can host the launch surface. Both `LaunchProgress::new`
/// and the pre-launch `prelaunch_select_choice` picker gate through this so
/// the message cannot drift between them.
pub(crate) fn require_rich_terminal() -> anyhow::Result<()> {
    if !rich_terminal_supported() {
        anyhow::bail!(
            "jackin load requires a rich terminal: stdin/stdout/stderr must be TTYs, TERM must not be dumb, CI must be unset, and the terminal must be at least 80x24"
        );
    }
    Ok(())
}

fn terminal_supports_rich_surface(require_stderr: bool) -> bool {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return false;
    }
    if require_stderr && !std::io::stderr().is_terminal() {
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
const LABEL_SIDE_OVERHANG: usize = 12;
const LABEL_EDGE_FADE_WIDTH: usize = 24;
const LABEL_SLIDE_FRAMES: usize = 12;
const PROGRESS_RAIL_WIDTH: usize =
    LaunchStage::ALL.len() * BLOCK_WIDTH + (LaunchStage::ALL.len() - 1) * BLOCK_GAP;
const LABEL_VIEW_WIDTH: usize = PROGRESS_RAIL_WIDTH + LABEL_SIDE_OVERHANG * 2;

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
        render_failure_popup(frame, area, view, failure, run_id);
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
    let label_width = usize::from(area.width).min(LABEL_VIEW_WIDTH);
    let lines = vec![
        blocks_line(view, frozen),
        labels_line(view, frozen, label_width),
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
        .map(|(index, row)| match index.cmp(&active) {
            std::cmp::Ordering::Less => {
                if row.status == StageStatus::Failed {
                    StageStatus::Failed
                } else {
                    StageStatus::Done
                }
            }
            std::cmp::Ordering::Equal => row.status,
            std::cmp::Ordering::Greater => StageStatus::Queued,
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
            StageStatus::Done | StageStatus::Skipped => ('━', PHOSPHOR_GREEN),
            StageStatus::Running => ('━', if pulse { WHITE } else { PHOSPHOR_GREEN }),
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
        let cell = if index >= 0 {
            strip
                .get(index as usize)
                .copied()
                .unwrap_or_else(blank_label_cell)
        } else {
            blank_label_cell()
        };
        faded_label_cell(cell, label_edge_fade_factor(x, width))
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

fn label_edge_fade_factor(index: usize, width: usize) -> f32 {
    let fade_width = LABEL_EDGE_FADE_WIDTH.min(width / 2).max(1);
    let edge_distance = index.min(width.saturating_sub(1).saturating_sub(index));
    if edge_distance >= fade_width {
        return 1.0;
    }

    let ratio = ((edge_distance + 1) as f32 / fade_width as f32).clamp(0.0, 1.0);
    ratio * ratio * 2.0f32.mul_add(-ratio, 3.0)
}

fn faded_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let factor = factor.clamp(0.0, 1.0);
            let scale = |c: u8| (f32::from(c) * factor) as u8;
            Color::Rgb(scale(r), scale(g), scale(b))
        }
        other => other,
    }
}

fn faded_label_cell(cell: LabelCell, factor: f32) -> LabelCell {
    let mut style = cell.style;
    if let Some(fg) = style.fg {
        style.fg = Some(faded_color(fg, factor));
    }
    LabelCell { style, ..cell }
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
    render_status_footer(
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

#[derive(Debug)]
struct FailurePopupRow {
    label: &'static str,
    value: String,
    copy_target: Option<FailureCopyTarget>,
}

fn failure_popup_rows(failure: &LaunchFailure, run_id: &str) -> Vec<FailurePopupRow> {
    let mut rows = vec![
        FailurePopupRow {
            label: "message",
            value: failure.summary.clone(),
            copy_target: None,
        },
        FailurePopupRow {
            label: "stage",
            value: failure.stage.label().to_string(),
            copy_target: None,
        },
        FailurePopupRow {
            label: "run id",
            value: run_id.to_string(),
            copy_target: Some(FailureCopyTarget::RunId),
        },
    ];
    if let Some(path) = &failure.diagnostics_path {
        rows.push(FailurePopupRow {
            label: "run diagnostics",
            value: path.display().to_string(),
            copy_target: Some(FailureCopyTarget::DiagnosticsPath),
        });
    }
    if let Some(path) = &failure.command_output_path {
        rows.push(FailurePopupRow {
            label: "docker output",
            value: path.display().to_string(),
            copy_target: Some(FailureCopyTarget::CommandOutputPath),
        });
    }
    if let Some(next) = &failure.next_step {
        rows.push(FailurePopupRow {
            label: "next",
            value: next.clone(),
            copy_target: None,
        });
    }
    rows
}

fn failure_popup_rect(area: Rect, row_count: usize) -> Rect {
    let popup_w = (area.width.saturating_mul(3) / 5)
        .clamp(40.min(area.width), area.width.saturating_sub(2).max(1));
    let height = u16::try_from(row_count)
        .unwrap_or(u16::MAX)
        .saturating_add(6)
        .min(area.height.saturating_sub(2).max(7));
    centered_rect(popup_w, height, area)
}

/// Inner body rect (inside the border, plus one column of padding) where the
/// failure rows render. Render and hit-testing derive geometry from this same
/// helper so the clickable value columns can never drift from what is drawn.
const fn failure_popup_body_rect(rect: Rect) -> Rect {
    let inner = rect.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    Rect {
        x: inner.x.saturating_add(1),
        y: inner.y.saturating_add(1),
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(3),
    }
}

fn failure_popup_value_rect(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
) -> Option<Rect> {
    let body = failure_popup_body_rect(rect);
    let label_width = FAILURE_POPUP_LABEL_WIDTH;
    rows.iter()
        .position(|row| row.copy_target == Some(target))
        .and_then(|idx| {
            if idx >= usize::from(body.height) {
                return None;
            }
            let x = body.x.saturating_add(
                u16::try_from(label_width + jackin_tui::display_cols(FAILURE_POPUP_SEP))
                    .unwrap_or(u16::MAX),
            );
            Some(Rect {
                x,
                y: body
                    .y
                    .saturating_add(u16::try_from(idx).unwrap_or(u16::MAX)),
                width: body.x.saturating_add(body.width).saturating_sub(x),
                height: 1,
            })
        })
}

fn failure_copy_target_at(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    col: u16,
    row: u16,
) -> Option<FailureCopyTarget> {
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect(area, rows.len());
    for entry in rows.iter().filter(|row| row.copy_target.is_some()) {
        let target = entry.copy_target?;
        let value_rect = failure_popup_value_rect(rect, &rows, target)?;
        let value_cols = u16::try_from(jackin_tui::display_cols(&entry.value)).unwrap_or(u16::MAX);
        let hit_width = value_rect.width.min(value_cols.max(1));
        if row == value_rect.y
            && col >= value_rect.x
            && col < value_rect.x.saturating_add(hit_width)
        {
            return Some(target);
        }
    }
    None
}

fn failure_copy_payload(
    failure: &LaunchFailure,
    run_id: &str,
    target: FailureCopyTarget,
) -> Option<String> {
    // Derive the copied value from the same `failure_popup_rows` builder the
    // renderer uses. Re-deriving paths/run-id here would duplicate the
    // formatting logic and drift if `failure_popup_rows` ever changes how it
    // displays a path (shell-escaping, `~`-collapse, etc.).
    failure_popup_rows(failure, run_id)
        .into_iter()
        .find(|row| row.copy_target == Some(target))
        .map(|row| row.value)
}

fn render_failure_popup_line(
    row: &FailurePopupRow,
    width: u16,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
) -> Line<'static> {
    let label = Style::default().fg(PHOSPHOR_DIM);
    let value_style = match row.copy_target {
        Some(target) if hovered == Some(target) => Style::default()
            .fg(LINK_BLUE)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        Some(_) => Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        None => Style::default().fg(WHITE),
    };
    let label_width = FAILURE_POPUP_LABEL_WIDTH;
    let badge = row
        .copy_target
        .filter(|target| copied == Some(*target))
        .map_or("", |_| "  Copied!");
    let fixed_cols =
        label_width + jackin_tui::display_cols(FAILURE_POPUP_SEP) + jackin_tui::display_cols(badge);
    let value_cols = usize::from(width).saturating_sub(fixed_cols);
    let value = jackin_tui::take_display_cols(&row.value, value_cols);
    let mut spans = vec![
        Span::styled(format!("{:<label_width$}", row.label), label),
        Span::styled(FAILURE_POPUP_SEP, Style::default().fg(PHOSPHOR_DARK)),
        Span::styled(value, value_style),
    ];
    if !badge.is_empty() {
        spans.push(Span::styled(
            badge,
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

const FAILURE_POPUP_LABEL_WIDTH: usize = 16;
/// Separator drawn between a row's label and value. The renderer paints
/// this string and the click hit-test uses its display width as the
/// label→value column offset, so the two cannot drift if the separator
/// is ever changed.
const FAILURE_POPUP_SEP: &str = " · ";

fn render_failure_popup(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    failure: &LaunchFailure,
    run_id: &str,
) {
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect(area, rows.len());
    let title = format!(" {} ", failure.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DANGER_RED))
        .title(Span::styled(
            title,
            Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let body = failure_popup_body_rect(rect);
    for (idx, row) in rows.iter().take(usize::from(body.height)).enumerate() {
        let line = render_failure_popup_line(
            row,
            body.width,
            view.failure_copy_hover,
            view.failure_copied,
        );
        let row_area = Rect {
            x: body.x,
            y: body.y + u16::try_from(idx).unwrap_or(u16::MAX),
            width: body.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(line), row_area);
    }

    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let button_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("  OK  ", focused_style)))
            .alignment(Alignment::Center),
        button_area,
    );
    // The popup draws no hint of its own (footer-only-hints rule); show the
    // dismiss keys on the bottom row, over the now-frozen status bar.
    let hint_row = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    render_hint_bar(frame, hint_row, FAILURE_HINT);
}

/// Footer-hint keys for the launch failure popup (dismiss only).
const FAILURE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("click"),
    HintSpan::Text("copy value"),
    HintSpan::GroupSep,
    HintSpan::Key("↵/Esc"),
    HintSpan::Text("dismiss"),
];

/// Footer-hint keys for the forced-choice launch picker.
const PICKER_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑/↓"),
    HintSpan::Text("navigate"),
    HintSpan::GroupSep,
    HintSpan::Text("type to filter"),
    HintSpan::GroupSep,
    HintSpan::Key("↵"),
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
/// Paint the shared solid dialog backdrop over `area` (capsule modal
/// convention — hide the cockpit, never dim it) and split off the bottom row
/// for the footer hint. Returns `(box_area, hint_area)` so every launch dialog
/// centers its box and renders its hint the same way.
fn dialog_backdrop(frame: &mut Frame<'_>, area: Rect) -> (Rect, Rect) {
    frame.render_widget(
        Block::default().style(Style::default().bg(DIALOG_BACKDROP)),
        area,
    );
    let box_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let hint_area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };
    (box_area, hint_area)
}

fn render_build_log_dialog(frame: &mut Frame<'_>, area: Rect, view: &LaunchView) {
    let (box_area, hint_area) = dialog_backdrop(frame, area);

    let title = if crate::runtime::build_log::is_active() {
        " Docker build · building… "
    } else {
        " Docker build "
    };
    // The full output drives the shared scrollable block so its proportional
    // scrollbar is correct. Cloning the (capped) buffer is acceptable here: the
    // overlay is a transient, operator-opened modal, not the steady cockpit.
    let raw = crate::runtime::build_log::snapshot();
    let viewport_w = viewport_width(box_area);
    let lines: Vec<Line<'_>> = if raw.is_empty() {
        vec![Line::from(Span::styled(
            "(waiting for docker build output…)",
            Style::default().fg(PHOSPHOR_DIM),
        ))]
    } else {
        wrap_build_log_lines(raw, viewport_w)
    };

    // `build_log_scroll` counts lines up from the tail (0 = follow newest).
    // Convert through the shared tail adapter to the block's top-offset.
    let viewport_h = viewport_height(box_area);
    let mut scroll_y = u16::try_from(view.build_log_scroll.to_top_offset(lines.len(), viewport_h))
        .unwrap_or(u16::MAX);
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

    render_hint_bar(frame, hint_area, BUILD_LOG_HINT);
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

    let default_style = Style::default().fg(Color::Gray).bg(DIALOG_SURFACE);
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
                Style::default().fg(PHOSPHOR_DIM).bg(DIALOG_SURFACE),
            ),
        );
    }
    lines.push(Line::from(spans));
}

fn draw_select(frame: &mut Frame<'_>, title: &str, context: &[Line<'_>], picker: &SelectListState) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_select_list(
        frame,
        picker_rect(box_area, picker, context),
        picker,
        title,
        context,
    );
    render_hint_bar(frame, hint_area, PICKER_HINT);
}

fn draw_text_prompt(frame: &mut Frame<'_>, input: &TextInputState<'_>, skippable: bool) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_text_input(frame, text_prompt_rect(box_area), input);
    render_hint_bar(frame, hint_area, text_prompt_hint(skippable));
}

fn draw_confirm(frame: &mut Frame<'_>, state: &ConfirmState) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_confirm_dialog(frame, confirm_rect(box_area, state), state);
    render_hint_bar(frame, hint_area, CONFIRM_HINT);
}

fn draw_error_popup(frame: &mut Frame<'_>, state: &ErrorPopupState) {
    let (box_area, hint_area) = dialog_backdrop(frame, frame.area());
    render_error_dialog(frame, error_popup_rect(box_area, state), state);
    render_hint_bar(frame, hint_area, ERROR_POPUP_HINT);
}

fn picker_rect(area: Rect, picker: &SelectListState, context: &[Line<'_>]) -> Rect {
    // Interior: filter row + spacer + one row per item, plus two borders; a
    // non-empty context block adds its line count plus a spacer.
    let context_rows = u16::try_from(context.len()).unwrap_or(u16::MAX);
    let context_extra = if context_rows > 0 {
        context_rows.saturating_add(1)
    } else {
        0
    };
    let rows = u16::try_from(picker.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .saturating_add(context_extra);
    let height = rows.clamp(6, area.height.saturating_sub(2).max(6));
    let min_w = 40.min(area.width);
    let max_w = (area.width.saturating_mul(4) / 5).max(min_w);
    let context_w = context
        .iter()
        .map(|line| u16::try_from(line.width()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0);
    let width = picker
        .max_label_width()
        .max(context_w)
        .saturating_add(6)
        .clamp(min_w, max_w);
    centered_rect(width, height, area)
}

fn text_prompt_rect(area: Rect) -> Rect {
    let min_w = 50.min(area.width);
    let width = (area.width.saturating_mul(3) / 5).clamp(min_w, area.width.max(min_w));
    centered_rect(width, 5, area)
}

fn confirm_rect(area: Rect, state: &ConfirmState) -> Rect {
    let width = area.width.saturating_mul(confirm_width_pct(state)) / 100;
    let height = confirm_required_height(state);
    centered_rect(width, height, area)
}

fn error_popup_rect(area: Rect, state: &ErrorPopupState) -> Rect {
    let width = (area.width.saturating_mul(3) / 4).clamp(40, area.width.max(40));
    let height = error_dialog_required_height(state, width.saturating_sub(2), area.height);
    centered_rect(width, height, area)
}

/// Footer-hint keys for the launch text prompt. `skippable` adds the
/// leave-empty-to-skip group; both share the rest of the vocabulary.
const fn text_prompt_hint(skippable: bool) -> &'static [HintSpan<'static>] {
    if skippable {
        TEXT_PROMPT_SKIP_HINT
    } else {
        TEXT_PROMPT_HINT
    }
}

const TEXT_PROMPT_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-C"),
    HintSpan::Text("cancel"),
];

const TEXT_PROMPT_SKIP_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↵"),
    HintSpan::Text("save"),
    HintSpan::GroupSep,
    HintSpan::Key("empty"),
    HintSpan::Text("skip"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-C"),
    HintSpan::Text("cancel"),
];

const CONFIRM_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Y"),
    HintSpan::Text("yes"),
    HintSpan::GroupSep,
    HintSpan::Key("N/Esc"),
    HintSpan::Text("no"),
    HintSpan::GroupSep,
    HintSpan::Key("⇥"),
    HintSpan::Text("focus"),
];

const ERROR_POPUP_HINT: &[HintSpan<'static>] = &[HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")];

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
            detail: None,
            next_step: None,
            stage: LaunchStage::Network,
            diagnostics_path: None,
            command_output_path: None,
        }
    }

    #[tokio::test]
    async fn stage_failed_does_not_block_on_test_renderer() {
        // The Rich path waits for an operator Enter/Esc dismiss. The test
        // renderer returns immediately so failure-state tests do not hang.
        let mut progress = LaunchProgress::for_test(test_diagnostics());
        tokio::time::timeout(
            std::time::Duration::from_millis(500),
            progress.stage_failed(dummy_failure()),
        )
        .await
        .expect("stage_failed must not block on the test renderer");
        assert!(progress.view.lock().unwrap().failure.is_some());
        assert!(!progress.view.lock().unwrap().failure_ack);
    }

    #[tokio::test]
    async fn stage_failed_writes_full_detail_to_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let run = RunDiagnostics::start(&paths, false, "load").unwrap();
        let mut progress = LaunchProgress::for_test(run.clone());

        progress
            .stage_failed(LaunchFailure {
                title: "Launch failed".to_string(),
                summary: "preparing kimi binary".to_string(),
                detail: Some(
                    "preparing kimi binary: resolving latest kimi binary: https://code.kimi.com/kimi-code/latest failed: curl: (28) Connection timed out after 30001 milliseconds"
                        .to_string(),
                ),
                next_step: None,
                stage: LaunchStage::DerivedImage,
                diagnostics_path: None,
                command_output_path: None,
            })
            .await;

        let body = std::fs::read_to_string(run.path()).unwrap();
        let events = body
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let event = events
            .iter()
            .find(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("stage_failed")
            })
            .unwrap();

        assert_eq!(
            event.get("message").and_then(serde_json::Value::as_str),
            Some("preparing kimi binary"),
        );
        assert_eq!(
            event.get("detail").and_then(serde_json::Value::as_str),
            Some(
                "preparing kimi binary: resolving latest kimi binary: https://code.kimi.com/kimi-code/latest failed: curl: (28) Connection timed out after 30001 milliseconds"
            ),
        );
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
    fn select_choice_errors_without_rich_renderer() {
        let mut progress = LaunchProgress::for_test(test_diagnostics());
        let error = progress
            .select_choice("pick", vec!["a".into(), "b".into()])
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires the rich launch dialog")
        );
    }

    #[test]
    fn env_prompts_error_without_rich_renderer() {
        let mut progress = LaunchProgress::for_test(test_diagnostics());

        assert!(
            progress
                .prompt_text("API key", None, true)
                .unwrap_err()
                .to_string()
                .contains("requires the rich launch dialog")
        );
        assert!(
            progress
                .prompt_select("Project", &["web".to_string()], None, false)
                .unwrap_err()
                .to_string()
                .contains("requires the rich launch dialog")
        );
    }

    #[test]
    fn text_prompt_dialog_renders_prompt_and_default() {
        let backend = TestBackend::new(90, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let input = TextInputState::new("Branch name", "main");

        terminal
            .draw(|frame| draw_text_prompt(frame, &input, false))
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Branch name"), "{rendered}");
        assert!(rendered.contains("main"), "{rendered}");
        assert!(rendered.contains("↵"), "{rendered}");
    }

    #[test]
    fn confirm_dialog_renders_role_trust_details() {
        let backend = TestBackend::new(100, 26);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = ConfirmState::role_trust(
            "acme/agent-jones",
            "https://github.com/acme/jackin-agent-jones.git",
        );

        terminal.draw(|frame| draw_confirm(frame, &state)).unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Trust role source"), "{rendered}");
        assert!(rendered.contains("acme/agent-jones"), "{rendered}");
        assert!(rendered.contains("jackin-agent-jones"), "{rendered}");
        assert!(rendered.contains('Y'), "{rendered}");
    }

    #[test]
    fn error_popup_dialog_renders_title_and_message() {
        let backend = TestBackend::new(100, 26);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = ErrorPopupState::new("Cleanup failed", "could not render the cleanup dialog");

        terminal
            .draw(|frame| draw_error_popup(frame, &state))
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Cleanup failed"), "{rendered}");
        assert!(
            rendered.contains("could not render the cleanup dialog"),
            "{rendered}"
        );
        assert!(rendered.contains("dismiss"), "{rendered}");
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
                "agent binaries",
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
            LaunchStage::AgentBinaries,
            StageStatus::Done,
            "cached",
        );
        update_stage(
            &mut view,
            LaunchStage::DerivedImage,
            StageStatus::Running,
            "building",
        );

        let statuses = display_stage_statuses(&view);
        assert_eq!(
            &statuses[..6],
            &[
                StageStatus::Done,
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
    fn stage_label_line_stays_near_the_progress_rail() {
        let mut view = initial_view();
        update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
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
            StageStatus::Running,
            "online",
        );

        let labels = labels_line(&view, true, LABEL_VIEW_WIDTH);
        let rendered = labels
            .spans
            .iter()
            .map(|span| &*span.content)
            .collect::<String>();
        let rendered_width = rendered.chars().count();
        assert_eq!(rendered_width, LABEL_VIEW_WIDTH);
        assert!(rendered_width > PROGRESS_RAIL_WIDTH);
        assert!(rendered.contains("credentials"), "{rendered}");
        assert!(rendered.contains("construct"), "{rendered}");
        assert!(rendered.contains("agent binaries"), "{rendered}");
    }

    #[test]
    fn label_edge_fade_factor_is_lower_at_the_edges() {
        let width = 24;
        let center = label_edge_fade_factor(width / 2, width);
        let left = label_edge_fade_factor(0, width);
        let right = label_edge_fade_factor(width - 1, width);

        assert!(center > 0.95, "center should stay nearly full brightness");
        assert!(left < 0.1, "left edge should almost disappear");
        assert!(right < 0.1, "right edge should almost disappear");
    }

    #[test]
    fn faded_color_scales_rgb_channels() {
        assert_eq!(
            faded_color(Color::Rgb(100, 200, 50), 0.5),
            Color::Rgb(50, 100, 25)
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
        assert!(jackin_tui::components::max_line_width(&lines) <= 32);
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
        let _guard = crate::runtime::build_log::TEST_LOCK.lock().unwrap();
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
            build_log_scroll: jackin_tui::scroll::TailScroll::default(),
            build_log_hover: false,
            label_transition: None,
            failure_copy_hover: None,
            failure_copied: None,
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
    fn build_log_scroll_down_from_saturated_top_moves_visible_content() {
        let _guard = crate::runtime::build_log::TEST_LOCK.lock().unwrap();
        crate::runtime::build_log::begin();
        for idx in 0..20 {
            crate::runtime::build_log::push_line(&format!("line {idx:02}"));
        }
        crate::runtime::build_log::end();

        let area = Rect::new(0, 0, 40, 8);
        let filled = build_log_scroll_filled(area);
        assert!(filled > 1);
        let mut view = LaunchView {
            identity: None,
            stages: Vec::new(),
            status: String::new(),
            failure: None,
            failure_ack: false,
            frame: 0,
            build_log_open: true,
            build_log_scroll: jackin_tui::scroll::TailScroll::new(usize::MAX),
            build_log_hover: false,
            label_transition: None,
            failure_copy_hover: None,
            failure_copied: None,
        };

        scroll_build_log(&mut view, area, -1);

        assert_eq!(view.build_log_scroll.offset(), filled - 1);
        assert_eq!(view.build_log_scroll.to_top_offset(20, 5), 1);
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
            build_log_scroll: jackin_tui::scroll::TailScroll::default(),
            build_log_hover: false,
            label_transition: None,
            failure_copy_hover: None,
            failure_copied: None,
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
            detail: None,
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

    fn failure_with_paths() -> LaunchFailure {
        use std::path::PathBuf;
        LaunchFailure {
            title: "Docker build failed".to_string(),
            summary: "Building the Docker container failed.".to_string(),
            detail: None,
            next_step: None,
            stage: LaunchStage::DerivedImage,
            diagnostics_path: Some(PathBuf::from("/jk/run/x.jsonl")),
            command_output_path: Some(PathBuf::from("/jk/run/x.docker-build.log")),
        }
    }

    #[test]
    fn failure_copy_target_at_hits_each_copyable_row_value() {
        // The whole point of the copy-on-click feature: a click landing on a
        // copyable value's drawn columns must register as that target. Render
        // and hit-test share `failure_popup_body_rect`, so this also pins the
        // "they cannot drift" invariant the helper's doc-comment claims.
        let area = Rect::new(0, 0, 80, 24);
        let failure = failure_with_paths();
        let run_id = "jk-run-testid";
        let rows = failure_popup_rows(&failure, run_id);
        let rect = failure_popup_rect(area, rows.len());

        for target in [
            FailureCopyTarget::RunId,
            FailureCopyTarget::DiagnosticsPath,
            FailureCopyTarget::CommandOutputPath,
        ] {
            let vr = failure_popup_value_rect(rect, &rows, target)
                .expect("copyable target must have a value rect");
            assert_eq!(
                failure_copy_target_at(area, &failure, run_id, vr.x, vr.y),
                Some(target),
                "click at value-column start must hit {target:?}",
            );
            // One column left of the value column lands in the label area —
            // must not register as a copy target.
            assert_eq!(
                failure_copy_target_at(area, &failure, run_id, vr.x.saturating_sub(1), vr.y),
                None,
                "click in label area must not hit {target:?}",
            );
        }
    }

    #[test]
    fn failure_copy_target_at_ignores_non_copyable_rows_and_absent_paths() {
        // The message row is non-copyable; a click on its y at the value
        // column must return None. An absent path produces no row, so its
        // value-rect lookup must return None too.
        let area = Rect::new(0, 0, 80, 24);
        let failure = LaunchFailure {
            command_output_path: None,
            ..failure_with_paths()
        };
        let run_id = "jk-run-x";
        let rows = failure_popup_rows(&failure, run_id);
        let rect = failure_popup_rect(area, rows.len());
        let run_id_rect = failure_popup_value_rect(rect, &rows, FailureCopyTarget::RunId).unwrap();
        // Rows: message=0, stage=1, run id=2. The message row sits two rows
        // above the run-id row in the body.
        let message_y = run_id_rect.y.saturating_sub(2);
        assert_eq!(
            failure_copy_target_at(area, &failure, run_id, run_id_rect.x, message_y),
            None,
            "click on the non-copyable message row must not hit any target",
        );
        assert!(
            failure_popup_value_rect(rect, &rows, FailureCopyTarget::CommandOutputPath).is_none(),
            "absent docker-output path must produce no value rect",
        );
    }

    #[test]
    fn failure_copy_payload_sources_value_from_rows() {
        // Single source of truth: the copied value must equal what the
        // renderer would show, sourced from `failure_popup_rows`. Re-deriving
        // here would drift if the row builder ever reformats paths.
        let failure = failure_with_paths();
        let run_id = "jk-run-payload";
        assert_eq!(
            failure_copy_payload(&failure, run_id, FailureCopyTarget::RunId).as_deref(),
            Some(run_id),
        );
        assert_eq!(
            failure_copy_payload(&failure, run_id, FailureCopyTarget::DiagnosticsPath).as_deref(),
            Some("/jk/run/x.jsonl"),
        );
        assert_eq!(
            failure_copy_payload(&failure, run_id, FailureCopyTarget::CommandOutputPath).as_deref(),
            Some("/jk/run/x.docker-build.log"),
        );
        let no_paths = LaunchFailure {
            diagnostics_path: None,
            command_output_path: None,
            ..failure_with_paths()
        };
        assert_eq!(
            failure_copy_payload(&no_paths, run_id, FailureCopyTarget::DiagnosticsPath),
            None,
            "absent path yields no payload",
        );
    }

    #[test]
    fn failure_popup_renders_copyable_rows_and_copied_badge() {
        let backend = TestBackend::new(120, 28);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut view = initial_view();
        view.failure = Some(failure_with_paths());
        view.failure_copied = Some(FailureCopyTarget::RunId);
        let run_id = "jk-run-rendered";
        terminal
            .draw(|frame| render_launch_frame(frame, &view, run_id, true, None))
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());

        for needle in [
            "run id",
            run_id,
            "run diagnostics",
            "/jk/run/x.jsonl",
            "docker output",
            "/jk/run/x.docker-build.log",
            "Copied!",    // badge next to the row whose target is `failure_copied`
            "copy value", // footer hint
        ] {
            assert!(
                rendered.contains(needle),
                "rendered failure popup must contain {needle:?}; got {rendered}",
            );
        }
    }
}
