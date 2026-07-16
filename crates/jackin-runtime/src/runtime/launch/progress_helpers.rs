// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch progress, prompt, and summary helpers.

pub(in crate::runtime) struct StepCounter {
    pub(in crate::runtime::launch) current: u32,
    pub(in crate::runtime::launch) role_name: String,
    pub(in crate::runtime::launch) current_stage: Option<crate::runtime::progress::LaunchStage>,
    pub(in crate::runtime::launch) progress: Option<crate::runtime::progress::LaunchProgress>,
    stage_telemetry: [Option<jackin_telemetry::launch::StageGuard>; 11],
    target_kind: jackin_telemetry::schema::enums::LaunchTargetKind,
}

impl StepCounter {
    pub(in crate::runtime::launch) fn new(
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

    pub(in crate::runtime::launch) fn start_progress(
        &mut self,
        progress: crate::runtime::progress::LaunchProgress,
    ) {
        self.progress = Some(progress);
    }

    pub(in crate::runtime::launch) async fn next(&mut self, text: &str) -> anyhow::Result<()> {
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
    pub(in crate::runtime::launch) fn is_cancelled(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| progress.cancel_token().is_cancelled())
    }

    pub(in crate::runtime::launch) fn done(&self) {
        jackin_diagnostics::set_terminal_title(&self.role_name);
    }

    pub(in crate::runtime::launch) const fn progress_mut(
        &mut self,
    ) -> Option<&mut crate::runtime::progress::LaunchProgress> {
        self.progress.as_mut()
    }

    pub(in crate::runtime::launch) fn stage_started(
        &mut self,
        stage: crate::runtime::progress::LaunchStage,
        detail: impl Into<String>,
    ) {
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

    pub(in crate::runtime::launch) fn stage_done(
        &mut self,
        stage: crate::runtime::progress::LaunchStage,
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

    pub(in crate::runtime::launch) fn stage_skipped(
        &mut self,
        stage: crate::runtime::progress::LaunchStage,
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

    pub(in crate::runtime::launch) async fn stage_failed(
        &mut self,
        failure: crate::runtime::progress::LaunchFailure,
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

    pub(in crate::runtime::launch) fn opening_hardline(&mut self) {
        self.stage_started(
            crate::runtime::progress::LaunchStage::Hardline,
            "opening hardline",
        );
    }

    fn finish_stage(
        &mut self,
        stage: crate::runtime::progress::LaunchStage,
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
    pub(in crate::runtime::launch) fn finish_progress(&mut self) {
        if let Some(progress) = self.progress.as_mut() {
            progress.finish();
        }
        self.progress = None;
    }
}

const fn stage_index(stage: crate::runtime::progress::LaunchStage) -> usize {
    match stage {
        crate::runtime::progress::LaunchStage::Identity => 0,
        crate::runtime::progress::LaunchStage::Role => 1,
        crate::runtime::progress::LaunchStage::Credentials => 2,
        crate::runtime::progress::LaunchStage::Construct => 3,
        crate::runtime::progress::LaunchStage::AgentBinaries => 4,
        crate::runtime::progress::LaunchStage::DerivedImage => 5,
        crate::runtime::progress::LaunchStage::Workspace => 6,
        crate::runtime::progress::LaunchStage::Network => 7,
        crate::runtime::progress::LaunchStage::Sidecar => 8,
        crate::runtime::progress::LaunchStage::Capsule => 9,
        crate::runtime::progress::LaunchStage::Hardline => 10,
    }
}

const fn telemetry_stage(
    stage: crate::runtime::progress::LaunchStage,
) -> jackin_telemetry::schema::enums::LaunchStageName {
    use jackin_telemetry::schema::enums::LaunchStageName as TelemetryStage;
    match stage {
        crate::runtime::progress::LaunchStage::Identity => TelemetryStage::Identity,
        crate::runtime::progress::LaunchStage::Role => TelemetryStage::Role,
        crate::runtime::progress::LaunchStage::Credentials => TelemetryStage::Credentials,
        crate::runtime::progress::LaunchStage::Construct => TelemetryStage::Construct,
        crate::runtime::progress::LaunchStage::AgentBinaries => TelemetryStage::AgentBinaries,
        crate::runtime::progress::LaunchStage::DerivedImage => TelemetryStage::DerivedImage,
        crate::runtime::progress::LaunchStage::Workspace => TelemetryStage::Workspace,
        crate::runtime::progress::LaunchStage::Network => TelemetryStage::Network,
        crate::runtime::progress::LaunchStage::Sidecar => TelemetryStage::Sidecar,
        crate::runtime::progress::LaunchStage::Capsule => TelemetryStage::Capsule,
        crate::runtime::progress::LaunchStage::Hardline => TelemetryStage::Hardline,
    }
}

pub(in crate::runtime::launch) struct LaunchEnvPrompter<'a> {
    progress: Option<std::cell::RefCell<&'a mut crate::runtime::progress::LaunchProgress>>,
}

impl<'a> LaunchEnvPrompter<'a> {
    pub(in crate::runtime::launch) fn new(
        progress: Option<&'a mut crate::runtime::progress::LaunchProgress>,
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

pub(in crate::runtime::launch) fn sensitive_mount_prompt(
    sensitive: &[jackin_config::SensitiveMount],
) -> String {
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

fn stage_for_step_text(text: &str) -> crate::runtime::progress::LaunchStage {
    match text {
        "Resolving role identity" => crate::runtime::progress::LaunchStage::Role,
        "Preparing runtime binaries" => crate::runtime::progress::LaunchStage::AgentBinaries,
        "Preparing derived image" => crate::runtime::progress::LaunchStage::DerivedImage,
        "Starting Docker-in-Docker" => crate::runtime::progress::LaunchStage::Sidecar,
        "Launching role" => crate::runtime::progress::LaunchStage::Capsule,
        _ => crate::runtime::progress::LaunchStage::Identity,
    }
}

const fn completion_label(stage: crate::runtime::progress::LaunchStage) -> &'static str {
    match stage {
        crate::runtime::progress::LaunchStage::Identity
        | crate::runtime::progress::LaunchStage::Credentials => "resolved",
        crate::runtime::progress::LaunchStage::Role => "trusted source",
        crate::runtime::progress::LaunchStage::Construct => "online",
        crate::runtime::progress::LaunchStage::AgentBinaries => "cached",
        crate::runtime::progress::LaunchStage::DerivedImage
        | crate::runtime::progress::LaunchStage::Capsule => "ready",
        crate::runtime::progress::LaunchStage::Workspace => "materialized",
        crate::runtime::progress::LaunchStage::Network => "isolated",
        crate::runtime::progress::LaunchStage::Sidecar => "awake",
        crate::runtime::progress::LaunchStage::Hardline => "open",
    }
}

pub(in crate::runtime::launch) const fn launch_target_kind(
    workspace_name: Option<&str>,
) -> crate::runtime::progress::LaunchTargetKind {
    if workspace_name.is_some() {
        crate::runtime::progress::LaunchTargetKind::Workspace
    } else {
        crate::runtime::progress::LaunchTargetKind::Directory
    }
}

pub(in crate::runtime::launch) fn launch_target_label(
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
pub(in crate::runtime::launch) fn launch_mount_lines(
    workspace: &jackin_config::ResolvedWorkspace,
) -> Vec<String> {
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
