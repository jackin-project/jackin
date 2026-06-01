use std::sync::Arc;
use std::time::Duration;

use jackin_tui::components::ConfirmState;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::renderer::RichRenderer;
use crate::tui::components::build_log_dialog::build_log_scroll_filled;
use crate::tui::subscriptions::{SharedView, handle_cockpit_input};
use crate::{
    LaunchDiagnostics, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchMessage,
    LaunchStage, PromptResult, StageStatus, initial_view, update_launch_view,
};

const STAGE_VISUAL_SETTLE: Duration = Duration::from_millis(140);

pub struct LaunchProgress {
    diagnostics: Arc<dyn LaunchDiagnostics>,
    renderer: Renderer,
    view: SharedView,
    host: &'static dyn LaunchHostTerminal,
}

enum Renderer {
    Rich(RichDriver),
    /// Rich surface torn down at the handoff; inert (no draws, no diagnostics
    /// trailer) so the interactive capsule attach owns the terminal alone.
    Done,
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
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
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
                    // Drain input only while this task owns the renderer. When
                    // a forced-choice picker holds it, the picker reads events
                    // itself and this poll would steal its keystrokes.
                    handle_cockpit_input(&view, &run_id, host, jackin_version);
                    let snapshot = match view.lock() {
                        Ok(mut v) => {
                            let build_log_filled = if v.build_log_open {
                                let area = crossterm::terminal::size()
                                    .ok()
                                    .map(|(width, height)| Rect::new(0, 0, width, height))
                                    .unwrap_or_default();
                                Some(build_log_scroll_filled(area))
                            } else {
                                None
                            };
                            let _dirty = update_launch_view(
                                &mut v,
                                LaunchMessage::RenderTick {
                                    advance_frame: !rr.no_motion(),
                                    build_log_filled,
                                },
                            );
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
    pub fn new(
        diagnostics: Arc<dyn LaunchDiagnostics>,
        no_motion: bool,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
    ) -> anyhow::Result<Self> {
        require_rich_terminal()?;
        let view: SharedView = Arc::new(std::sync::Mutex::new(initial_view()));
        let rich = RichRenderer::enter(no_motion, host, jackin_version)?;
        let renderer = Renderer::Rich(RichDriver::spawn(
            rich,
            view.clone(),
            diagnostics.run_id().to_string(),
            diagnostics.path().display().to_string(),
            host,
            jackin_version,
        ));
        Ok(Self {
            diagnostics,
            renderer,
            view,
            host,
        })
    }

    #[doc(hidden)]
    pub fn for_test(diagnostics: Arc<dyn LaunchDiagnostics>) -> Self {
        Self {
            diagnostics,
            renderer: Renderer::Test,
            view: Arc::new(std::sync::Mutex::new(initial_view())),
            host: crate::test_support::test_host_terminal(),
        }
    }

    #[doc(hidden)]
    pub fn view_for_test(&self) -> &SharedView {
        &self.view
    }

    pub fn run_id(&self) -> &str {
        self.diagnostics.run_id()
    }

    fn update_view(&self, msg: LaunchMessage) {
        if let Ok(mut view) = self.view.lock() {
            let _dirty = update_launch_view(&mut view, msg);
        }
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
            self.host.set_rich_surface_active(false);
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
    ) -> anyhow::Result<PromptResult> {
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
    ) -> anyhow::Result<PromptResult> {
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

impl Drop for LaunchProgress {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        // Dropped without an explicit finish (e.g. an error path): stop the
        // render task. Its renderer drops when the task exits, restoring the
        // terminal — the host-screen guard is the ultimate safety net.
        if let Renderer::Rich(driver) = &self.renderer {
            driver.stop.store(true, Ordering::Relaxed);
            self.host.set_rich_surface_active(false);
        }
    }
}

pub fn prelaunch_select_choice(
    no_motion: bool,
    title: &str,
    items: Vec<String>,
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<usize> {
    require_rich_terminal()?;
    let mut renderer = RichRenderer::enter(no_motion, host, jackin_version)?;
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
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<usize> {
    let mut renderer = RichRenderer::enter_dialog(false, host, jackin_version)?;
    renderer.select_with_context(title, context, items)
}

/// Standalone error popup for launch-adjacent failures that need operator
/// acknowledgement in the same rich surface.
pub fn standalone_error_popup(
    title: &str,
    message: &str,
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<()> {
    let mut renderer = RichRenderer::enter_dialog(false, host, jackin_version)?;
    renderer.error_popup(title, message)
}

/// Bail with the canonical rich-terminal requirement message unless the
/// current terminal can host the launch surface. Both `LaunchProgress::new`
/// and the pre-launch `prelaunch_select_choice` picker gate through this so
/// the message cannot drift between them.
pub fn require_rich_terminal() -> anyhow::Result<()> {
    if !crate::tui::terminal::rich_terminal_supported() {
        anyhow::bail!(
            "jackin load requires a rich terminal: stdin/stdout/stderr must be TTYs, TERM must not be dumb, CI must be unset, and the terminal must be at least 80x24"
        );
    }
    Ok(())
}
