//! Launch progress display: stage tracking and rich terminal progress bar
//! for the `jackin load` cockpit.
//!
//! Not responsible for: executing launch stages (see `runtime`) or
//! capsule attach after handoff.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::tui::run::{RichDriver, RichRenderer};
use crate::tui::subscriptions::SharedView;
use crate::{
    LaunchDiagnostics, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchMessage,
    LaunchStage, PromptContextLine, PromptResult, StageStatus, initial_view, update_launch_view,
};

const STAGE_VISUAL_SETTLE: Duration = Duration::from_millis(140);

#[expect(
    missing_debug_implementations,
    reason = "LaunchProgress owns terminal and diagnostics trait objects that do not expose useful Debug output."
)]
pub struct LaunchProgress {
    diagnostics: Arc<dyn LaunchDiagnostics>,
    renderer: Renderer,
    view: SharedView,
    host: &'static dyn LaunchHostTerminal,
    cancel_token: CancellationToken,
}

enum Renderer {
    Rich(RichDriver),
    /// Rich surface torn down at the handoff; inert (no draws, no diagnostics
    /// trailer) so the interactive capsule attach owns the terminal alone.
    Done,
    Test,
}

impl LaunchProgress {
    pub fn new(
        diagnostics: Arc<dyn LaunchDiagnostics>,
        no_motion: bool,
        host: &'static dyn LaunchHostTerminal,
        jackin_version: &'static str,
    ) -> anyhow::Result<Self> {
        crate::tui::terminal::require_rich_terminal()?;
        let cancel_token = CancellationToken::new();
        let view: SharedView = Arc::new(std::sync::Mutex::new(initial_view()));
        let rich = RichRenderer::enter(no_motion, host, jackin_version)?;
        let renderer = Renderer::Rich(RichDriver::spawn(
            rich,
            Arc::clone(&view),
            diagnostics.run_id().to_owned(),
            diagnostics
                .persists()
                .then(|| diagnostics.path().display().to_string()),
            host,
            jackin_version,
            cancel_token.clone(),
        ));
        Ok(Self {
            diagnostics,
            renderer,
            view,
            host,
            cancel_token,
        })
    }

    #[doc(hidden)]
    pub fn for_test(diagnostics: Arc<dyn LaunchDiagnostics>) -> Self {
        Self {
            diagnostics,
            renderer: Renderer::Test,
            view: Arc::new(std::sync::Mutex::new(initial_view())),
            host: crate::test_support::test_host_terminal(),
            cancel_token: CancellationToken::new(),
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

    fn emit_stage(
        &mut self,
        stage: LaunchStage,
        status: StageStatus,
        kind: &str,
        detail: impl Into<String>,
    ) {
        let detail = detail.into();
        // The activity spinner tracks in-progress stages: set it iff the stage is
        // still running. Done/Skipped are terminal and clear it.
        let set_activity = matches!(status, StageStatus::Running);
        self.update_view(LaunchMessage::StageStatus {
            stage,
            status,
            detail: detail.clone(),
            set_activity,
        });
        self.diagnostics.stage(kind, stage.label(), &detail, None);
    }

    pub fn stage_started(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        self.emit_stage(stage, StageStatus::Running, "stage_started", detail);
    }

    pub fn stage_progress(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        self.emit_stage(stage, StageStatus::Running, "stage_progress", detail);
    }

    pub fn stage_done(&mut self, stage: LaunchStage, detail: impl Into<String>) {
        self.emit_stage(stage, StageStatus::Done, "stage_done", detail);
    }

    pub fn stage_skipped(&mut self, stage: LaunchStage, reason: impl Into<String>) {
        self.emit_stage(stage, StageStatus::Skipped, "stage_skipped", reason);
    }

    pub async fn stage_failed(&mut self, mut failure: LaunchFailure) {
        let stage = failure.stage;
        let summary = failure.summary.clone();
        let next_step = failure.next_step.clone();
        let detail = failure.detail.clone();
        failure.diagnostics_path = self
            .diagnostics
            .persists()
            .then(|| self.diagnostics.path().to_path_buf());
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
        self.diagnostics.error("launch_failed", &summary, None);
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
        if let Renderer::Rich(driver) = &mut self.renderer {
            // Signal the task to stop drawing; it exits on its next tick and
            // drops its renderer (any stray final frame is wiped by the
            // capsule's clear-on-attach). Detach the handle — we do not block.
            driver.stop_detached();
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
            driver.with_renderer(f)
        } else {
            anyhow::bail!(crate::tui::run::rich_launch_dialog_required_message(what))
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
            renderer.confirm_prompt(prompt)
        })
    }

    pub fn confirm_role_trust(
        &mut self,
        role: impl Into<String>,
        repository: impl Into<String>,
    ) -> anyhow::Result<bool> {
        self.with_rich_renderer("role trust prompt", |renderer| {
            renderer.confirm_role_trust(role, repository)
        })
    }

    /// Returns a clone of the cancellation token so callers in `jackin-runtime`
    /// can register additional cancel-aware tasks without owning the token.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    pub async fn while_waiting<T, F>(&self, future: F) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        tokio::select! {
            result = future => result,
            () = self.cancel_token.cancelled() => {
                Err(crate::LaunchCancelled::err())
            }
        }
    }
}

impl Drop for LaunchProgress {
    fn drop(&mut self) {
        // Dropped without an explicit finish (e.g. an error path): stop the
        // render task. Its renderer drops when the task exits, restoring the
        // terminal — the host-screen guard is the ultimate safety net.
        if let Renderer::Rich(driver) = &self.renderer {
            driver.request_stop();
            self.host.set_rich_surface_active(false);
        }
    }
}

#[cfg(test)]
mod tests;

pub fn prelaunch_select_choice(
    no_motion: bool,
    title: &str,
    items: Vec<String>,
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<usize> {
    crate::tui::terminal::require_rich_terminal()?;
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
    context: &[PromptContextLine],
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

/// D23/D24: exit dialog with `I`-key inspect support.
///
/// Shows the D23 three-way choice (Return/Keep/Discard) with each preserved
/// worktree's file list reachable via `I`. `worktrees_per_record` maps 1:1
/// to the context records — one `Vec<WorktreeInspect>` per preserved record.
pub fn standalone_exit_dialog_with_inspect(
    title: &str,
    context: &[PromptContextLine],
    options: Vec<String>,
    worktrees_per_record: &[Vec<crate::WorktreeInspect>],
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<usize> {
    let mut renderer = RichRenderer::enter_dialog(false, host, jackin_version)?;
    renderer.exit_dialog_with_inspect(title, context, options, worktrees_per_record)
}

/// D23/D21: standalone launch dialog. Supports delete-in-place and D24 inspect.
///
/// Returns `LaunchDialogResult` which the caller processes (delete → purge,
/// then call again; restore → connect; fresh → supersede old candidates).
pub fn standalone_launch_dialog(
    title: &str,
    candidates: &[crate::LaunchCandidate],
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<crate::LaunchDialogResult> {
    let mut renderer = RichRenderer::enter_dialog(false, host, jackin_version)?;
    renderer.launch_dialog(title, candidates)
}

impl LaunchProgress {
    /// D23 launch dialog through the live launch progress surface.
    pub fn launch_dialog_progress(
        &mut self,
        title: &str,
        candidates: &[crate::LaunchCandidate],
    ) -> anyhow::Result<crate::LaunchDialogResult> {
        self.with_rich_renderer("launch dialog", |renderer| {
            renderer.launch_dialog(title, candidates)
        })
    }
}
