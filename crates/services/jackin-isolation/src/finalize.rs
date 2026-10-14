// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::print_stderr,
    reason = "isolation finalization emits operator-visible preservation and cleanup warnings"
)]

use crate::cleanup::force_cleanup_isolated;
use crate::state::{CleanupStatus, IsolationRecord, read_records, upsert_record};
use jackin_config::DirtyExitPolicy;
use jackin_core::PromptContextLine;
use jackin_core::error_popup;
use jackin_core::exit_dialog_with_inspect;
use jackin_core::{CommandRunner, ContainerHandle};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachOutcome {
    StillRunning,
    Stopped(i32),
    OomKilled,
}

impl AttachOutcome {
    pub const fn still_running() -> Self {
        Self::StillRunning
    }
    pub const fn stopped(code: i32) -> Self {
        Self::Stopped(code)
    }
    pub const fn oom_killed() -> Self {
        Self::OomKilled
    }

    pub fn as_label(self) -> String {
        match self {
            Self::StillRunning => "still_running".to_owned(),
            Self::Stopped(code) => format!("stopped_{code}"),
            Self::OomKilled => "oom_killed".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeDecision {
    Preserved,
    Cleaned,
    ReturnToAgent,
}

impl FinalizeDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::Cleaned => "cleaned",
            Self::ReturnToAgent => "return_to_agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreservedReason {
    Dirty,
    Unpushed,
}

impl PreservedReason {
    fn describe(self) -> &'static str {
        match self {
            Self::Dirty => "uncommitted changes",
            Self::Unpushed => "unpushed commits on a local branch",
        }
    }

    fn describe_terse(self) -> &'static str {
        match self {
            Self::Dirty => "uncommitted changes",
            Self::Unpushed => "unpushed commits",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDialogChoice {
    ReturnToRole,
    KeepAll,
    DiscardAll,
}

pub trait FinalizerPrompt {
    fn ask_exit_dialog(
        &mut self,
        container: &str,
        records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice>;
}

#[derive(Debug)]
pub struct RichCleanupPrompt;
impl FinalizerPrompt for RichCleanupPrompt {
    fn ask_exit_dialog(
        &mut self,
        container: &str,
        records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        Ok(rich_exit_dialog(container, records))
    }
}

#[derive(Debug)]
pub struct ExitActionPrompt {
    pub state_dir: std::path::PathBuf,
}

impl FinalizerPrompt for ExitActionPrompt {
    fn ask_exit_dialog(
        &mut self,
        _container: &str,
        _records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        Ok(match read_exit_action(&self.state_dir) {
            Some(jackin_protocol::ExitAction::Discard) => ExitDialogChoice::DiscardAll,
            Some(jackin_protocol::ExitAction::Keep) | None => ExitDialogChoice::KeepAll,
        })
    }
}

pub(crate) fn read_exit_action(state_dir: &Path) -> Option<jackin_protocol::ExitAction> {
    let path = state_dir.join(jackin_protocol::EXIT_ACTION_FILENAME);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim()).ok()
}

fn rich_exit_dialog(
    container: &str,
    records: &[(IsolationRecord, PreservedReason)],
) -> ExitDialogChoice {
    let worktrees_per_record: Vec<Vec<jackin_core::WorktreeInspect>> = records
        .iter()
        .map(|(rec, _)| vec![crate::git_inspect::worktree_inspect(&rec.worktree_path)])
        .collect();

    let mut context = vec![
        PromptContextLine::Emphasis(format!(
            "Container {container} exited with unsaved isolated work."
        )),
        PromptContextLine::Blank,
    ];
    for (rec, reason) in records {
        let reason_tag = reason.describe_terse();
        context.push(PromptContextLine::Path(rec.worktree_path.clone()));
        context.push(PromptContextLine::Muted(format!("  ({reason_tag})")));
    }
    context.push(PromptContextLine::Blank);
    context.push(PromptContextLine::Muted(
        "Choose how jackin❯ should handle these worktrees. Press I to inspect changes.".to_owned(),
    ));

    let options = vec![
        "Return to role to address it".to_owned(),
        "Keep all and exit".to_owned(),
        "Discard all and exit".to_owned(),
    ];

    match exit_dialog_with_inspect(
        "Isolated Worktrees",
        &context,
        options,
        &worktrees_per_record,
    ) {
        Ok(0) => ExitDialogChoice::ReturnToRole,
        Ok(2) => ExitDialogChoice::DiscardAll,
        Ok(_) => ExitDialogChoice::KeepAll,
        Err(err) => {
            let message = format!(
                "Container {container} has unsaved isolated work.\n\nCould not render the exit dialog:\n{err:#}\n\nAll worktrees will be preserved."
            );
            drop(error_popup("Exit Dialog Error", &message));
            ExitDialogChoice::KeepAll
        }
    }
}

pub struct FinalizeContext<'a, P, D, R> {
    pub container_name: &'a str,
    pub container_state_dir: &'a Path,
    pub outcome: AttachOutcome,
    pub is_interactive: bool,
    pub dirty_exit_policy: DirtyExitPolicy,
    pub prompt: &'a mut P,
    pub docker: &'a D,
    pub runner: &'a mut R,
    pub container: ContainerHandle,
}

impl<P, D, R> std::fmt::Debug for FinalizeContext<'_, P, D, R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinalizeContext")
            .field("container_name", &self.container_name)
            .field("container_state_dir", &self.container_state_dir)
            .field("outcome", &self.outcome)
            .field("is_interactive", &self.is_interactive)
            .field("dirty_exit_policy", &self.dirty_exit_policy)
            .field("container", &self.container)
            .finish_non_exhaustive()
    }
}

pub async fn finalize_foreground_session<P, D, R>(
    context: FinalizeContext<'_, P, D, R>,
) -> anyhow::Result<FinalizeDecision>
where
    P: FinalizerPrompt,
    D: jackin_docker::docker_client::DockerApi,
    R: CommandRunner,
{
    let FinalizeContext {
        container_name,
        container_state_dir,
        outcome,
        is_interactive,
        dirty_exit_policy,
        prompt,
        docker,
        runner,
        container,
    } = context;
    if !matches!(outcome, AttachOutcome::Stopped(0)) {
        let no_sessions = !has_jackin_sessions_by_id(docker, &container).await;
        if matches!(outcome, AttachOutcome::StillRunning) && no_sessions {
            return finalize_clean_exit(
                container_name,
                container_state_dir,
                is_interactive,
                dirty_exit_policy,
                prompt,
                runner,
            )
            .await;
        }
        return Ok(FinalizeDecision::Preserved);
    }
    finalize_clean_exit(
        container_name,
        container_state_dir,
        is_interactive,
        dirty_exit_policy,
        prompt,
        runner,
    )
    .await
}

async fn has_jackin_sessions_by_id(
    docker: &impl jackin_docker::docker_client::DockerApi,
    container: &ContainerHandle,
) -> bool {
    let status_command = jackin_core::jackin_status_command(
        jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR,
    );
    match docker
        .exec_capture_by_id(container, &["sh", "-c", &status_command])
        .await
    {
        Ok(output) => match jackin_core::parse_session_count(&output) {
            Some(0) => false,
            Some(_) => true,
            None => {
                eprintln!(
                    "[jackin] warning: could not parse jackin session status in {}; \
                     treating as sessions-present — run `jackin purge {}` to clean \
                     up isolation worktrees if this was a clean exit",
                    container.name(),
                    container.name()
                );
                true
            }
        },
        Err(e) => {
            eprintln!(
                "[jackin] warning: could not check jackin sessions in {} ({e}); \
                 treating as sessions-present — run `jackin purge {}` to clean \
                 up isolation worktrees if this was a clean exit",
                container.name(),
                container.name()
            );
            true
        }
    }
}

async fn finalize_clean_exit(
    container_name: &str,
    container_state_dir: &Path,
    is_interactive: bool,
    dirty_exit_policy: DirtyExitPolicy,
    prompt: &mut impl FinalizerPrompt,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    let records = read_records(container_state_dir)?;
    let mut preserved_records: Vec<(IsolationRecord, PreservedReason)> = Vec::new();

    for record in records {
        let assessment = assess_cleanup(&record, runner).await?;
        match assessment {
            CleanupAssessment::SafeToDelete => {
                force_cleanup_isolated(&record, container_state_dir, runner).await?;
            }
            CleanupAssessment::PreservedDirty => {
                let record =
                    mark_preserved(container_state_dir, &record, CleanupStatus::PreservedDirty)?;
                preserved_records.push((record, PreservedReason::Dirty));
            }
            CleanupAssessment::PreservedUnpushed => {
                let record = mark_preserved(
                    container_state_dir,
                    &record,
                    CleanupStatus::PreservedUnpushed,
                )?;
                preserved_records.push((record, PreservedReason::Unpushed));
            }
        }
    }

    if preserved_records.is_empty() {
        return Ok(FinalizeDecision::Cleaned);
    }

    match dirty_exit_policy {
        DirtyExitPolicy::Discard => {
            let mut any_failed = false;
            for (rec, reason) in &preserved_records {
                if let Err(e) = force_cleanup_isolated(rec, container_state_dir, runner).await {
                    let reason_str = reason.describe();
                    eprintln!(
                        "[jackin] warning: discard-policy force-delete of `{wt}` failed ({reason_str}): {e}\n         re-run `jackin purge {short}` to retry",
                        wt = rec.worktree_path,
                        short = jackin_core::instance_id_from_container_base(container_name)
                            .unwrap_or(container_name),
                    );
                    any_failed = true;
                }
            }
            return if any_failed {
                Ok(FinalizeDecision::Preserved)
            } else {
                Ok(FinalizeDecision::Cleaned)
            };
        }
        DirtyExitPolicy::Keep => {
            for (rec, reason) in &preserved_records {
                let reason_str = reason.describe();
                eprintln!(
                    "[jackin] preserved isolated worktree for {container_name} (keep policy):\n         {wt}\n         reason: {reason_str}",
                    wt = rec.worktree_path,
                );
            }
            return Ok(FinalizeDecision::Preserved);
        }
        DirtyExitPolicy::Ask => {}
    }

    if !is_interactive {
        for (rec, reason) in &preserved_records {
            let reason_str = reason.describe();
            eprintln!(
                "[jackin] preserved isolated worktree for {container_name}:\n         {wt}\n         reason: {reason_str}\n         run `jackin hardline {short}` to return, inspect the path above directly, or `jackin purge {short}` to discard",
                wt = rec.worktree_path,
                short = jackin_core::instance_id_from_container_base(container_name)
                    .unwrap_or(container_name),
            );
        }
        return Ok(FinalizeDecision::Preserved);
    }

    match prompt.ask_exit_dialog(container_name, &preserved_records)? {
        ExitDialogChoice::ReturnToRole => Ok(FinalizeDecision::ReturnToAgent),
        ExitDialogChoice::KeepAll => Ok(FinalizeDecision::Preserved),
        ExitDialogChoice::DiscardAll => {
            let mut any_failed = false;
            for (rec, _reason) in preserved_records {
                if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner).await {
                    eprintln!(
                        "[jackin] warning: force-delete of isolated worktree `{wt}` failed: {e}\n         record retained — re-run `jackin purge {short}` to retry",
                        wt = rec.worktree_path,
                        short = jackin_core::instance_id_from_container_base(container_name)
                            .unwrap_or(container_name),
                    );
                    any_failed = true;
                }
            }
            if any_failed {
                Ok(FinalizeDecision::Preserved)
            } else {
                Ok(FinalizeDecision::Cleaned)
            }
        }
    }
}

#[derive(Debug)]
enum CleanupAssessment {
    SafeToDelete,
    PreservedDirty,
    PreservedUnpushed,
}

async fn assess_cleanup(
    record: &IsolationRecord,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<CleanupAssessment> {
    let state =
        jackin_core::assess_worktree(&record.worktree_path, &record.base_commit, runner, |_| {})
            .await?;
    Ok(match state {
        jackin_core::WorktreeState::Clean => CleanupAssessment::SafeToDelete,
        jackin_core::WorktreeState::Dirty => CleanupAssessment::PreservedDirty,
        jackin_core::WorktreeState::Unpushed => CleanupAssessment::PreservedUnpushed,
    })
}

fn mark_preserved(
    container_state_dir: &Path,
    record: &IsolationRecord,
    status: CleanupStatus,
) -> anyhow::Result<IsolationRecord> {
    let mut updated = record.clone();
    updated.cleanup_status = status;
    upsert_record(container_state_dir, updated.clone())?;
    Ok(updated)
}

#[cfg(test)]
mod tests;
