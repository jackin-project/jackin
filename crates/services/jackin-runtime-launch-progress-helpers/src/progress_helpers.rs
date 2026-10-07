// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch progress, prompt, and summary helpers.

#[expect(
    missing_debug_implementations,
    reason = "StepCounter owns a LaunchProgress whose terminal and diagnostics handles do not expose useful Debug output."
)]
pub struct StepCounter {
    pub current: u32,
    pub role_name: String,
    pub current_stage: Option<jackin_runtime_progress::progress::LaunchStage>,
    pub progress: Option<jackin_runtime_progress::progress::LaunchProgress>,
    stage_telemetry: [Option<jackin_telemetry::launch::StageGuard>; 11],
    target_kind: jackin_telemetry::schema::enums::LaunchTargetKind,
}

impl StepCounter {
    pub fn new(
        role_name: &str,
        target_kind: jackin_telemetry::schema::enums::LaunchTargetKind,
    ) -> Self {
        Self {
            current: 0,
            role_name: role_name.to_owned(),
            current_stage: None,
            progress: None,
            stage_telemetry: std::array::from_fn(|_| None),
            target_kind,
        }
    }

    pub fn start_progress(&mut self, progress: jackin_runtime_progress::progress::LaunchProgress) {
        self.progress = Some(progress);
    }

    pub async fn next(&mut self, text: &str) -> anyhow::Result<()> {
        // Step boundaries are cancellation checkpoints. Long blocking ops are
        // each raced against the token via the progress `while_waiting` seam,
        // but the quick async work *between* them (docker inspects, cache
        // probes) is not individually raced; bailing here bounds how long a
        // Ctrl+C can go unobserved to a single step. The bail unwinds through
        // the pipeline's normal `Err` cleanup, same as any leaf race.
        if self.is_cancelled() {
            return Err(jackin_core::LaunchCancelled::err());
        }
        if let Some(stage) = self.current_stage {
            self.stage_done(stage, completion_label(stage));
        }
        self.current += 1;
        jackin_diagnostics::set_terminal_title(&format!("{} \u{2014} {text}", self.role_name));
        let stage = stage_for_step_text(text);
        self.current_stage = Some(stage);
        self.stage_started(stage, text);
        if let Some(progress) = &self.progress {
            progress.settle_stage_visual().await;
        }
        Ok(())
    }

    /// `true` once the operator has hit Ctrl+C / Ctrl+Q on the rich launch
    /// surface. Always `false` in the headless (no-progress) path, where
    /// cancellation is the OS's SIGINT rather than the cockpit's token.
    pub fn is_cancelled(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| progress.cancel_token().is_cancelled())
    }

    pub fn done(&self) {
        jackin_diagnostics::set_terminal_title(&self.role_name);
    }

    pub const fn progress_mut(
        &mut self,
    ) -> Option<&mut jackin_runtime_progress::progress::LaunchProgress> {
        self.progress.as_mut()
    }

    pub fn stage_started(
        &mut self,
        stage: jackin_runtime_progress::progress::LaunchStage,
        detail: impl Into<String>,
    ) {
        if self.current_stage == Some(stage) {
            self.current_stage = None;
        }
        let index = stage_index(stage);
        if let Some(previous) = self.stage_telemetry[index].take() {
            previous.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Cancellation,
                None,
            );
        }
        self.stage_telemetry[index] = Some(jackin_telemetry::launch::StageGuard::start(
            telemetry_stage(stage),
            self.target_kind,
        ));
        if let Some(progress) = &mut self.progress {
            progress.stage_started(stage, detail);
        }
    }

    pub fn stage_done(
        &mut self,
        stage: jackin_runtime_progress::progress::LaunchStage,
        detail: impl Into<String>,
    ) {
        self.finish_stage(
            stage,
            jackin_telemetry::schema::enums::OutcomeValue::Success,
            None,
        );
        if let Some(progress) = &mut self.progress {
            progress.stage_done(stage, detail);
        }
    }

    pub fn stage_skipped(
        &mut self,
        stage: jackin_runtime_progress::progress::LaunchStage,
        reason: impl Into<String>,
    ) {
        self.finish_stage(
            stage,
            jackin_telemetry::schema::enums::OutcomeValue::Skip,
            None,
        );
        if let Some(progress) = &mut self.progress {
            progress.stage_skipped(stage, reason);
        }
    }

    pub async fn stage_failed(
        &mut self,
        failure: jackin_runtime_progress::progress::LaunchFailure,
    ) {
        self.finish_stage(
            failure.stage,
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::LaunchStageFailed),
        );
        if let Some(progress) = &mut self.progress {
            progress.stage_failed(failure).await;
        }
    }

    pub fn stage_error(&mut self, stage: jackin_runtime_progress::progress::LaunchStage) {
        self.finish_stage(
            stage,
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::LaunchStageFailed),
        );
    }

    pub fn opening_hardline(&mut self) {
        self.stage_started(
            jackin_runtime_progress::progress::LaunchStage::Hardline,
            "opening hardline",
        );
    }

    fn finish_stage(
        &mut self,
        stage: jackin_runtime_progress::progress::LaunchStage,
        outcome: jackin_telemetry::schema::enums::OutcomeValue,
        error_type: Option<jackin_telemetry::schema::enums::ErrorType>,
    ) {
        let index = stage_index(stage);
        let telemetry = self.stage_telemetry[index].take().unwrap_or_else(|| {
            jackin_telemetry::launch::StageGuard::start(telemetry_stage(stage), self.target_kind)
        });
        telemetry.complete(outcome, error_type);
    }

    /// Stop the rich loading surface's render task and clear
    /// `rich_surface_active`. Call this before handing the terminal to an
    /// interactive `docker exec -it` session, otherwise the capsule attach
    /// can't own the PTY and hangs.
    pub fn finish_progress(&mut self) {
        if let Some(progress) = self.progress.as_mut() {
            progress.finish();
        }
        self.progress = None;
    }
}

const fn stage_index(stage: jackin_runtime_progress::progress::LaunchStage) -> usize {
    match stage {
        jackin_runtime_progress::progress::LaunchStage::Identity => 0,
        jackin_runtime_progress::progress::LaunchStage::Role => 1,
        jackin_runtime_progress::progress::LaunchStage::Credentials => 2,
        jackin_runtime_progress::progress::LaunchStage::Construct => 3,
        jackin_runtime_progress::progress::LaunchStage::AgentBinaries => 4,
        jackin_runtime_progress::progress::LaunchStage::DerivedImage => 5,
        jackin_runtime_progress::progress::LaunchStage::Workspace => 6,
        jackin_runtime_progress::progress::LaunchStage::Network => 7,
        jackin_runtime_progress::progress::LaunchStage::Sidecar => 8,
        jackin_runtime_progress::progress::LaunchStage::Capsule => 9,
        jackin_runtime_progress::progress::LaunchStage::Hardline => 10,
    }
}

const fn telemetry_stage(
    stage: jackin_runtime_progress::progress::LaunchStage,
) -> jackin_telemetry::schema::enums::LaunchStageName {
    use jackin_telemetry::schema::enums::LaunchStageName as TelemetryStage;
    match stage {
        jackin_runtime_progress::progress::LaunchStage::Identity => TelemetryStage::Identity,
        jackin_runtime_progress::progress::LaunchStage::Role => TelemetryStage::Role,
        jackin_runtime_progress::progress::LaunchStage::Credentials => TelemetryStage::Credentials,
        jackin_runtime_progress::progress::LaunchStage::Construct => TelemetryStage::Construct,
        jackin_runtime_progress::progress::LaunchStage::AgentBinaries => {
            TelemetryStage::AgentBinaries
        }
        jackin_runtime_progress::progress::LaunchStage::DerivedImage => {
            TelemetryStage::DerivedImage
        }
        jackin_runtime_progress::progress::LaunchStage::Workspace => TelemetryStage::Workspace,
        jackin_runtime_progress::progress::LaunchStage::Network => TelemetryStage::Network,
        jackin_runtime_progress::progress::LaunchStage::Sidecar => TelemetryStage::Sidecar,
        jackin_runtime_progress::progress::LaunchStage::Capsule => TelemetryStage::Capsule,
        jackin_runtime_progress::progress::LaunchStage::Hardline => TelemetryStage::Hardline,
    }
}

#[expect(
    missing_debug_implementations,
    reason = "LaunchEnvPrompter borrows a LaunchProgress whose terminal and diagnostics handles do not expose useful Debug output."
)]
pub struct LaunchEnvPrompter<'a> {
    progress: Option<std::cell::RefCell<&'a mut jackin_runtime_progress::progress::LaunchProgress>>,
}

impl<'a> LaunchEnvPrompter<'a> {
    pub fn new(
        progress: Option<&'a mut jackin_runtime_progress::progress::LaunchProgress>,
    ) -> Self {
        Self {
            progress: progress.map(std::cell::RefCell::new),
        }
    }
}

impl jackin_env::EnvPrompter for LaunchEnvPrompter<'_> {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<jackin_env::PromptResult> {
        if let Some(progress) = &self.progress {
            return progress.borrow_mut().prompt_text(title, default, skippable);
        }
        anyhow::bail!("manifest env text prompt requires the rich launch dialog")
    }

    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<jackin_env::PromptResult> {
        if let Some(progress) = &self.progress {
            return progress
                .borrow_mut()
                .prompt_select(title, options, default, skippable);
        }
        anyhow::bail!("manifest env select prompt requires the rich launch dialog")
    }
}

pub fn sensitive_mount_prompt(sensitive: &[jackin_config::SensitiveMount]) -> String {
    let mut lines = vec![
        "Sensitive host paths are mounted into this role container.".to_owned(),
        "Continue only if this role should see these credentials.".to_owned(),
        String::new(),
    ];
    for hit in sensitive {
        lines.push(format!("{} — {}", hit.src, hit.reason));
    }
    lines.push(String::new());
    lines.push("Continue with these mounts?".to_owned());
    lines.join("\n")
}

fn stage_for_step_text(text: &str) -> jackin_runtime_progress::progress::LaunchStage {
    match text {
        "Resolving role identity" => jackin_runtime_progress::progress::LaunchStage::Role,
        "Preparing runtime binaries" => {
            jackin_runtime_progress::progress::LaunchStage::AgentBinaries
        }
        "Preparing derived image" => jackin_runtime_progress::progress::LaunchStage::DerivedImage,
        "Starting Docker-in-Docker" => jackin_runtime_progress::progress::LaunchStage::Sidecar,
        "Launching role" => jackin_runtime_progress::progress::LaunchStage::Capsule,
        _ => jackin_runtime_progress::progress::LaunchStage::Identity,
    }
}

const fn completion_label(stage: jackin_runtime_progress::progress::LaunchStage) -> &'static str {
    match stage {
        jackin_runtime_progress::progress::LaunchStage::Identity
        | jackin_runtime_progress::progress::LaunchStage::Credentials => "resolved",
        jackin_runtime_progress::progress::LaunchStage::Role => "trusted source",
        jackin_runtime_progress::progress::LaunchStage::Construct => "online",
        jackin_runtime_progress::progress::LaunchStage::AgentBinaries => "cached",
        jackin_runtime_progress::progress::LaunchStage::DerivedImage
        | jackin_runtime_progress::progress::LaunchStage::Capsule => "ready",
        jackin_runtime_progress::progress::LaunchStage::Workspace => "materialized",
        jackin_runtime_progress::progress::LaunchStage::Network => "isolated",
        jackin_runtime_progress::progress::LaunchStage::Sidecar => "awake",
        jackin_runtime_progress::progress::LaunchStage::Hardline => "open",
    }
}

pub const fn launch_target_kind(
    workspace_name: Option<&str>,
) -> jackin_runtime_progress::progress::LaunchTargetKind {
    if workspace_name.is_some() {
        jackin_runtime_progress::progress::LaunchTargetKind::Workspace
    } else {
        jackin_runtime_progress::progress::LaunchTargetKind::Directory
    }
}

pub fn launch_target_label(
    workspace_name: Option<&str>,
    workspace: &jackin_config::ResolvedWorkspace,
) -> String {
    workspace_name.map_or_else(
        || jackin_diagnostics::shorten_home(&workspace.workdir),
        str::to_owned,
    )
}

/// Human-readable lines for the mounts whose host source differs from the
/// container destination. Same-path mounts (the current-directory launch
/// case) carry no information for the operator and are omitted entirely, so
/// a directory launch shows no mount line at all.
pub fn launch_mount_lines(workspace: &jackin_config::ResolvedWorkspace) -> Vec<String> {
    workspace
        .mounts
        .iter()
        .filter(|mount| mount.src.trim_end_matches('/') != mount.dst.trim_end_matches('/'))
        .map(|mount| {
            let ro = if mount.readonly { " (ro)" } else { "" };
            format!(
                "{} → {}{ro}",
                jackin_diagnostics::shorten_home(&mount.src),
                mount.dst
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
