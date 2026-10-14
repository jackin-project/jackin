// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
            host: crate::fixtures::test_host_terminal(),
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
        let set_activity = matches!(status, StageStatus::Running);
        self.update_view(LaunchMessage::StageStatus {
            stage,
            status,
            detail: detail.clone(),
            set_activity,
        });
        self.diagnostics.stage(kind, stage, &detail, None);
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

    pub async fn stage_failed(&mut self, failure: LaunchFailure) {
        let stage = failure.stage;
        let summary = failure.summary.clone();
        let next_step = failure.next_step.clone();
        let detail = failure.detail.clone();
        self.update_view(LaunchMessage::StageFailed(failure));
        self.diagnostics.stage(
            "stage_failed",
            stage,
            &summary,
            detail.as_deref().or(next_step.as_deref()),
        );
        self.diagnostics.error("launch_failed", &summary, None);
        if matches!(self.renderer, Renderer::Rich(_)) {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if failure_acknowledged(&self.view) {
                    break;
                }
            }
        }
    }

    pub fn opening_hardline(&mut self) {
        self.stage_started(LaunchStage::Hardline, "opening hardline");
    }

    pub async fn settle_stage_visual(&self) {
        if matches!(self.renderer, Renderer::Rich(_)) {
            tokio::time::sleep(STAGE_VISUAL_SETTLE).await;
        }
    }

    pub fn finish(&mut self) {
        if let Renderer::Rich(driver) = &mut self.renderer {
            driver.stop_detached();
            self.host.set_rich_surface_active(false);
            self.renderer = Renderer::Done;
        }
    }

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
        if let Renderer::Rich(driver) = &self.renderer {
            driver.request_stop();
            self.host.set_rich_surface_active(false);
        }
    }
}

fn failure_acknowledged(view: &SharedView) -> bool {
    match view.lock() {
        Ok(view) => view.failure_ack,
        Err(poisoned) => poisoned.into_inner().failure_ack,
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

pub fn standalone_error_popup(
    title: &str,
    message: &str,
    host: &'static dyn LaunchHostTerminal,
    jackin_version: &'static str,
) -> anyhow::Result<()> {
    let mut renderer = RichRenderer::enter_dialog(false, host, jackin_version)?;
    renderer.error_popup(title, message)
}

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
