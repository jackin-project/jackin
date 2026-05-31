use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use crossterm::ExecutableCommand;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use jackin_tui::ModalOutcome;
use jackin_tui::components::{
    ConfirmState, ErrorPopupState, SelectListState, TextInputState, status_footer_right_chip_rect,
};
#[cfg(test)]
use jackin_tui::theme::DANGER_RED;
use ratatui::Frame;
use ratatui::layout::Rect;
#[cfg(test)]
use ratatui::style::Color;
use ratatui::text::Line;

#[cfg(test)]
use jackin_launch::tui::build_log::BUILD_LOG_WRAP_PREFIX;
use jackin_launch::tui::build_log::{build_log_scroll_filled, scroll_build_log};
#[cfg(test)]
use jackin_launch::tui::build_log::{render_build_log_dialog, wrap_build_log_lines};
use jackin_launch::tui::cockpit::render_launch_frame as render_launch_frame_view;
use jackin_launch::tui::container_info::{launch_container_info_rect, launch_container_info_state};
use jackin_launch::tui::failure::{
    failure_copy_payload, failure_copy_target_at, failure_popup_hyperlink_overlay,
};
#[cfg(test)]
use jackin_launch::tui::failure::{
    failure_popup_rect_for_rows, failure_popup_rows, failure_popup_value_rect,
};
use jackin_launch::tui::footer::{footer_instance, format_activity};
#[cfg(test)]
use jackin_launch::tui::progress::{
    LABEL_SLIDE_FRAMES, LABEL_VIEW_WIDTH, PROGRESS_RAIL_WIDTH, animated_label_center,
    display_stage_statuses, faded_color, label_edge_fade_factor, label_strip, labels_line,
};
use jackin_launch::tui::prompts::{draw_confirm, draw_error_popup, draw_select, draw_text_prompt};
pub use jackin_launch::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchMessage, LaunchStage, LaunchTargetKind,
    LaunchView, StageLabelTransition, StageStatus, StageView, active_stage_index, initial_view,
    update_launch_view, update_stage,
};
use jackin_launch::{LaunchDiagnostics, LaunchHostTerminal};

impl LaunchDiagnostics for crate::diagnostics::RunDiagnostics {
    fn run_id(&self) -> &str {
        self.run_id()
    }

    fn path(&self) -> &std::path::Path {
        self.path()
    }

    fn command_output_path(&self, name: &str) -> std::path::PathBuf {
        self.command_output_path(name)
    }

    fn compact(&self, kind: &str, message: &str) {
        self.compact(kind, message);
    }

    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>) {
        self.stage(kind, stage, message, detail);
    }
}

struct HostTerminal;

impl LaunchHostTerminal for HostTerminal {
    fn set_rich_surface_active(&self, active: bool) {
        crate::tui::set_rich_surface_active(active);
    }

    fn host_screen_owned(&self) -> bool {
        crate::tui::host_screen_owned()
    }

    fn is_debug_mode(&self) -> bool {
        crate::tui::is_debug_mode()
    }

    fn emit_compact_line(&self, kind: &str, line: &str) {
        crate::tui::emit_compact_line(kind, line);
    }
}

static HOST_TERMINAL: HostTerminal = HostTerminal;

fn host_terminal() -> &'static dyn LaunchHostTerminal {
    &HOST_TERMINAL
}

type SharedView = Arc<std::sync::Mutex<LaunchView>>;
const STAGE_VISUAL_SETTLE: Duration = Duration::from_millis(140);

pub struct LaunchProgress {
    diagnostics: Arc<dyn LaunchDiagnostics>,
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
    fn spawn(
        renderer: RichRenderer,
        view: SharedView,
        run_id: String,
        run_log_path: String,
    ) -> Self {
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
                    let _ = rr.render(&snapshot, &run_id, &run_log_path);
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

impl LaunchProgress {
    pub fn new(diagnostics: Arc<dyn LaunchDiagnostics>, no_motion: bool) -> anyhow::Result<Self> {
        require_rich_terminal()?;
        let view: SharedView = Arc::new(std::sync::Mutex::new(initial_view()));
        let rich = RichRenderer::enter(no_motion)?;
        let renderer = Renderer::Rich(RichDriver::spawn(
            rich,
            view.clone(),
            diagnostics.run_id().to_string(),
            diagnostics.path().display().to_string(),
        ));
        Ok(Self {
            diagnostics,
            renderer,
            view,
        })
    }

    #[cfg(test)]
    pub fn for_test(diagnostics: Arc<dyn LaunchDiagnostics>) -> Self {
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
            host_terminal().set_rich_surface_active(false);
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
    ) -> anyhow::Result<jackin_launch::PromptResult> {
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
    ) -> anyhow::Result<jackin_launch::PromptResult> {
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

fn hit_footer_container_chip(
    view: &LaunchView,
    run_id: &str,
    area: Rect,
    col: u16,
    row: u16,
) -> bool {
    if view.build_log_open || view.failure.is_some() {
        return false;
    }
    let instance = footer_instance(view);
    if instance.is_empty() {
        return false;
    }
    let debug_chip = host_terminal().is_debug_mode().then_some(run_id);
    status_footer_right_chip_rect(
        Rect {
            x: 0,
            y: area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        },
        &instance,
        debug_chip,
    )
    .is_some_and(|rect| {
        row >= rect.y
            && row < rect.y.saturating_add(rect.height)
            && col >= rect.x
            && col < rect.x.saturating_add(rect.width)
    })
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

fn handle_cockpit_mouse_down(v: &mut LaunchView, area: Rect, run_id: &str, col: u16, row: u16) {
    if v.container_info_open {
        let state = launch_container_info_state(
            v,
            run_id,
            "",
            host_terminal().is_debug_mode(),
            env!("JACKIN_VERSION"),
        );
        let rect = launch_container_info_rect(area, &state);
        if let Some((row, payload)) =
            jackin_tui::components::container_info_copy_payload_at(rect, &state, col, row)
        {
            let mut out = std::io::stdout();
            let copy_ok = out
                .write_all(&jackin_tui::ansi::encode_osc52_clipboard_write(&payload))
                .and_then(|()| out.flush())
                .is_ok();
            if copy_ok {
                v.container_info_copied = Some(row);
            }
        } else {
            v.container_info_open = false;
            v.container_info_copied = None;
        }
    } else if let Some(failure) = v.failure.as_ref() {
        if let Some(target) = failure_copy_target_at(area, failure, run_id, col, row)
            && let Some(payload) = failure_copy_payload(failure, run_id, target)
        {
            let mut out = std::io::stdout();
            let copy_ok = out
                .write_all(&jackin_tui::ansi::encode_osc52_clipboard_write(&payload))
                .and_then(|()| out.flush())
                .is_ok();
            if copy_ok {
                v.failure_copied = Some(target);
            } else {
                host_terminal().emit_compact_line(
                    "failure-popup-copy",
                    "OSC 52 clipboard write failed — badge suppressed",
                );
            }
        }
    } else if v.build_log_open {
        v.build_log_open = false;
    } else if hit_footer_container_chip(v, run_id, area, col, row) {
        v.container_info_open = true;
        v.container_info_copied = None;
        v.footer_hover.right = false;
        set_cockpit_pointer(false);
    } else if jackin_launch::build_log::len() > 0 && hit_activity(v, col, row) {
        v.build_log_open = true;
        v.build_log_scroll = jackin_tui::scroll::TailScroll::default();
        v.footer_hover.left = false;
        set_cockpit_pointer(false);
    }
}

fn handle_cockpit_mouse_move(v: &mut LaunchView, area: Rect, run_id: &str, col: u16, row: u16) {
    if v.container_info_open {
        return;
    }
    if let Some(failure) = v.failure.as_ref() {
        let hover = failure_copy_target_at(area, failure, run_id, col, row);
        if hover != v.failure_copy_hover {
            v.failure_copy_hover = hover;
            set_cockpit_pointer(hover.is_some());
        }
        return;
    }
    let activity_hovering =
        !v.build_log_open && jackin_launch::build_log::len() > 0 && hit_activity(v, col, row);
    let container_hovering = hit_footer_container_chip(v, run_id, area, col, row);
    if activity_hovering != v.footer_hover.left || container_hovering != v.footer_hover.right {
        v.footer_hover.left = activity_hovering;
        v.footer_hover.right = container_hovering;
        set_cockpit_pointer(activity_hovering || container_hovering);
    }
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
                    handle_cockpit_mouse_down(&mut v, area, run_id, m.column, m.row);
                }
                MouseEventKind::Moved => {
                    handle_cockpit_mouse_move(&mut v, area, run_id, m.column, m.row);
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
                    && v.container_info_open
                    && matches!(k.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) =>
            {
                v.container_info_open = false;
                v.footer_hover.right = false;
                set_cockpit_pointer(false);
            }
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
            host_terminal().set_rich_surface_active(false);
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
    rain: Option<jackin_launch::tui::rain::RainState>,
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
        let entered_alt_screen = !host_terminal().host_screen_owned();
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
        host_terminal().set_rich_surface_active(true);
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

    fn render(
        &mut self,
        view: &LaunchView,
        run_id: &str,
        run_log_path: &str,
    ) -> anyhow::Result<()> {
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
                self.rain = Some(jackin_launch::tui::rain::RainState::new(cols, rows));
            }
            if !no_motion
                && !view.frame.is_multiple_of(3)
                && let Some(rain) = &mut self.rain
            {
                jackin_launch::tui::rain::tick_rain(rain);
            }
        }
        let rain = self.rain.as_ref();
        let size = self.terminal.size().ok();
        self.terminal
            .draw(|frame| render_launch_frame(frame, view, run_id, run_log_path, no_motion, rain))
            .map(|_| ())
            .context("rendering launch progress TUI")?;
        if let Some(size) = size {
            emit_failure_popup_hyperlink_overlay(
                Rect::new(0, 0, size.width, size.height),
                view,
                run_id,
            );
            emit_launch_container_info_hyperlink_overlay(
                Rect::new(0, 0, size.width, size.height),
                view,
                run_id,
                run_log_path,
            );
        }
        Ok(())
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
    ) -> anyhow::Result<jackin_launch::PromptResult> {
        self.with_raw_mode("entering raw mode for launch env prompt", |renderer| {
            renderer.prompt_text_loop(title, initial, skippable)
        })
    }

    fn prompt_text_loop(
        &mut self,
        title: &str,
        initial: &str,
        skippable: bool,
    ) -> anyhow::Result<jackin_launch::PromptResult> {
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
                        return Ok(jackin_launch::PromptResult::Skipped);
                    }
                    ModalOutcome::Commit(value) => {
                        return Ok(jackin_launch::PromptResult::Value(value));
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
    ) -> anyhow::Result<jackin_launch::PromptResult> {
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
    ) -> anyhow::Result<jackin_launch::PromptResult> {
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
                        return Ok(jackin_launch::PromptResult::Skipped);
                    }
                    ModalOutcome::Commit(index) => {
                        return Ok(jackin_launch::PromptResult::Value(options[index].clone()));
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
        host_terminal().set_rich_surface_active(false);
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

fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    no_motion: bool,
    rain: Option<&jackin_launch::tui::rain::RainState>,
) {
    render_launch_frame_view(
        frame,
        view,
        run_id,
        run_log_path,
        no_motion,
        rain,
        host_terminal().is_debug_mode(),
        env!("JACKIN_VERSION"),
    );
}

fn emit_launch_container_info_hyperlink_overlay(
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
) {
    if !view.container_info_open || view.failure.is_some() || view.build_log_open {
        return;
    }
    let state = launch_container_info_state(
        view,
        run_id,
        run_log_path,
        host_terminal().is_debug_mode(),
        env!("JACKIN_VERSION"),
    );
    let rect = launch_container_info_rect(area, &state);
    let overlay = jackin_tui::components::container_info_hyperlink_overlay(rect, &state);
    if overlay.is_empty() {
        return;
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(&overlay);
    let _ = out.flush();
}

fn emit_failure_popup_hyperlink_overlay(area: Rect, view: &LaunchView, run_id: &str) {
    let Some(failure) = view.failure.as_ref() else {
        return;
    };
    let overlay = failure_popup_hyperlink_overlay(
        area,
        failure,
        run_id,
        view.failure_copy_hover,
        view.failure_copied,
    );
    if overlay.is_empty() {
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(&overlay);
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::diagnostics::RunDiagnostics;
    use jackin_tui::components::StatusFooterHover;
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
        let _guard = jackin_launch::build_log::TEST_LOCK.lock().unwrap();
        jackin_launch::build_log::begin();
        jackin_launch::build_log::push_line(
            "#4 FROM docker.io/projectjackin/jackin-the-architect:latest@sha256:08d62f4027f941d8f5ee1742b6b0ba9e8a3e276ab7626967b0e1de27917a0e94",
        );
        jackin_launch::build_log::end();

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
            footer_hover: StatusFooterHover::default(),
            label_transition: None,
            failure_copy_hover: None,
            failure_copied: None,
            container_info_open: false,
            container_info_copied: None,
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
        let _guard = jackin_launch::build_log::TEST_LOCK.lock().unwrap();
        jackin_launch::build_log::begin();
        for idx in 0..20 {
            jackin_launch::build_log::push_line(&format!("line {idx:02}"));
        }
        jackin_launch::build_log::end();

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
            footer_hover: StatusFooterHover::default(),
            label_transition: None,
            failure_copy_hover: None,
            failure_copied: None,
            container_info_open: false,
            container_info_copied: None,
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
            footer_hover: StatusFooterHover::default(),
            label_transition: None,
            failure_copy_hover: None,
            failure_copied: None,
            container_info_open: false,
            container_info_copied: None,
        };
        terminal
            .draw(|frame| {
                render_launch_frame(
                    frame,
                    &view,
                    "jk-run-42f9aa",
                    "/tmp/jk-run-42f9aa.jsonl",
                    true,
                    None,
                );
            })
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
            .draw(|frame| {
                render_launch_frame(
                    frame,
                    &view,
                    "jk-run-42f9aa",
                    "/tmp/jk-run-42f9aa.jsonl",
                    true,
                    None,
                );
            })
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
        let rect = failure_popup_rect_for_rows(area, &rows);

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
    fn launch_container_info_renders_from_footer_chip_state() {
        crate::tui::set_debug_mode(true);
        let backend = TestBackend::new(100, 28);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut view = initial_view();
        view.identity = Some(LaunchIdentity {
            role: "agent-smith".to_string(),
            agent: "codex".to_string(),
            target_kind: LaunchTargetKind::Workspace,
            target_label: "big-monorepo".to_string(),
            mounts: Vec::new(),
            image: None,
            container: Some("jk-k7p9m2xq-bigmonorepo-agentsmith".to_string()),
        });
        view.container_info_open = true;
        terminal
            .draw(|frame| {
                render_launch_frame(
                    frame,
                    &view,
                    "jk-run-rendered",
                    "/tmp/jk-run-rendered.jsonl",
                    true,
                    None,
                );
            })
            .unwrap();
        crate::tui::set_debug_mode(false);

        let rendered = format!("{:?}", terminal.backend().buffer());
        for needle in [
            "Container info",
            "jk-k7p9m2xq-bigmonorepo-agentsmith",
            "jackin version",
            "agent-smith",
            "jk-run-rendered",
            "/tmp/jk-run-rendered.jsonl",
        ] {
            assert!(
                rendered.contains(needle),
                "container info dialog must contain {needle:?}: {rendered}"
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
        let rect = failure_popup_rect_for_rows(area, &rows);
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
            .draw(|frame| render_launch_frame(frame, &view, run_id, "/tmp/run.jsonl", true, None))
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

    #[test]
    fn failure_popup_wraps_long_paths_without_dropping_tail() {
        let backend = TestBackend::new(72, 28);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let long_path = "/jk/run/very/deep/path/with/a/long/docker-build-output-file.jsonl";
        let mut view = initial_view();
        view.failure = Some(LaunchFailure {
            diagnostics_path: Some(PathBuf::from(long_path)),
            ..failure_with_paths()
        });
        terminal
            .draw(|frame| {
                render_launch_frame(
                    frame,
                    &view,
                    "jk-run-rendered",
                    "/tmp/jk-run-rendered.jsonl",
                    true,
                    None,
                );
            })
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());

        for needle in ["/jk/run/very/deep/pa", "r-build-output-file.", "jsonl"] {
            assert!(
                rendered.contains(needle),
                "long path segment must remain visible instead of being silently truncated: {rendered}"
            );
        }
    }

    #[test]
    fn failure_popup_path_overlay_emits_osc8_file_links() {
        let area = Rect::new(0, 0, 120, 28);
        let failure = failure_with_paths();
        let overlay =
            failure_popup_hyperlink_overlay(area, &failure, "jk-run-rendered", None, None);
        let text = String::from_utf8_lossy(&overlay);

        assert!(text.contains("\x1b]8;;file:///jk/run/x.jsonl\x1b\\"));
        assert!(text.contains("\x1b]8;;file:///jk/run/x.docker-build.log\x1b\\"));
        assert!(text.contains("\x1b]8;;\x1b\\"));
    }
}
