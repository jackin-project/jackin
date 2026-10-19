// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! C7 Markdown export: a readable run report with plan summary,
//! candidate IDs, findings, test results, backup state, and operator
//! decisions. Credential material and raw secret-bearing state are
//! excluded: every free-text field passes through the existing
//! secret-redaction helpers, and terminal text is included only with
//! explicit content selection (`include_terminal`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use jackin_workflow::RunDetail;
use serde::Serialize;

use jackin_workflow::spool::{SpoolRead, sanitize_terminal_text};

/// What the export wrote: the report path plus the manifest the CLI
/// prints (stable JSON via `OutputEnvelope` at the call site).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WorkflowExportManifest {
    pub run_id: String,
    pub report_path: PathBuf,
    pub included_terminal: bool,
    pub event_count: usize,
    pub evidence_count: usize,
    pub checkpoint_count: usize,
}

/// Optional terminal text for the report. Raw escape sequences are
/// sanitized, and an observation gap stays visible instead of
/// claiming a lossless transcript.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExportTerminal<'a> {
    pub read: SpoolRead<'a>,
}

/// Render the Markdown report for a run detail. `terminal` selects
/// the sensitive terminal artifact explicitly; pass `None` for the
/// default export without terminal text.
#[must_use]
pub(crate) fn render_report(detail: &RunDetail, terminal: Option<ExportTerminal<'_>>) -> String {
    let summary = &detail.summary;
    let mut report = String::new();
    let _unused = writeln!(report, "# Workflow run {}", redact(&summary.run_id));
    let _unused = writeln!(report);
    let _unused = writeln!(report, "- plan: {}", redact(&summary.plan_hash));
    let _unused = writeln!(report, "- status: {}", redact(&summary.status));
    let _unused = writeln!(report, "- stage: {}", redact(&summary.stage));
    let _unused = writeln!(
        report,
        "- round: {}/{}",
        summary.round, summary.max_repair_turns
    );
    let _unused = writeln!(
        report,
        "- candidate: {}",
        summary.candidate.as_deref().map_or_else(
            || "none".to_owned(),
            |candidate| redact(candidate).into_owned()
        )
    );
    let _unused = writeln!(
        report,
        "- owner epoch: {} (version {})",
        summary.owner_epoch, summary.version
    );
    if let Some(pause_kind) = summary.pause_kind.as_deref() {
        let _unused = writeln!(report, "- paused as: {}", redact(pause_kind));
    }
    let _unused = writeln!(report);
    let _unused = writeln!(report, "## Sessions");
    if detail.sessions.is_empty() {
        let _unused = writeln!(report, "No recorded sessions.");
    }
    for session in &detail.sessions {
        let _unused = writeln!(
            report,
            "- {}: instance={}, session={}, epoch={}, identity={}",
            redact(&session.worker_id),
            redact_opt(session.instance_id.as_deref()),
            session
                .session_id
                .map_or_else(|| "none".to_owned(), |id| id.to_string()),
            session
                .capsule_epoch
                .map_or_else(|| "none".to_owned(), |epoch| epoch.to_string()),
            redact_opt(session.process_identity.as_deref()),
        );
    }
    let _unused = writeln!(report);
    let _unused = writeln!(report, "## Effects and backup state");
    if detail.effects.is_empty() {
        let _unused = writeln!(report, "No recorded effects.");
    }
    for effect in &detail.effects {
        let _unused = writeln!(
            report,
            "- {} {}: status={}, external={}, preservation={}",
            redact(&effect.kind),
            redact(&effect.operation_id),
            redact(&effect.status),
            redact_opt(effect.external_identity.as_deref()),
            redact_opt(effect.preservation.as_deref()),
        );
    }
    let _unused = writeln!(report);
    let _unused = writeln!(report, "## Evidence (test and review)");
    if detail.evidence.is_empty() {
        let _unused = writeln!(report, "No recorded evidence.");
    }
    for evidence in &detail.evidence {
        let _unused = writeln!(
            report,
            "- {} {}: source={}, quality={}, result={}",
            redact(&evidence.kind),
            redact(&evidence.candidate),
            redact(&evidence.source),
            redact(&evidence.quality),
            redact(&evidence.result),
        );
    }
    let _unused = writeln!(report);
    let _unused = writeln!(report, "## Checkpoints and preserved work");
    if detail.checkpoints.is_empty() {
        let _unused = writeln!(report, "No recorded checkpoints.");
    }
    for checkpoint in &detail.checkpoints {
        let _unused = writeln!(
            report,
            "- {}: commit={}, backup={}, path={}",
            redact(&checkpoint.candidate),
            redact(&checkpoint.commit_hash),
            redact(&checkpoint.backup_state),
            redact(&checkpoint.local_path),
        );
        if let Some(hold) = checkpoint.preservation_hold.as_deref() {
            let _unused = writeln!(report, "  preservation hold: {}", redact(hold));
        }
    }
    let _unused = writeln!(report);
    let _unused = writeln!(report, "## Event history");
    if detail.events.is_empty() {
        let _unused = writeln!(report, "No recorded events in this window.");
    }
    for event in &detail.events {
        let _unused = writeln!(
            report,
            "- seq={} {} by {}",
            event.seq,
            redact(&event.kind),
            redact(&event.actor)
        );
    }
    let _unused = writeln!(report);
    match terminal {
        Some(selected) => {
            let _unused = writeln!(report, "## Terminal (explicitly selected)");
            if selected.read.gap {
                let _unused = writeln!(
                    report,
                    "> Observation gap: {} byte(s) of output fell outside the retained spool window. This transcript is not lossless.",
                    selected.read.dropped_bytes
                );
            }
            let _unused = writeln!(report, "```text");
            report.push_str(&sanitize_terminal_text(selected.read.text));
            if !selected.read.text.ends_with('\n') {
                report.push('\n');
            }
            let _unused = writeln!(report, "```");
        }
        None => {
            let _unused = writeln!(
                report,
                "Terminal output excluded (default export omits sensitive terminal artifacts)."
            );
        }
    }
    report
}

fn redact(value: &str) -> std::borrow::Cow<'_, str> {
    jackin_diagnostics::redact::redact_text(value)
}

fn redact_opt(value: Option<&str>) -> String {
    value.map_or_else(|| "none".to_owned(), |text| redact(text).into_owned())
}

/// Write the report into `out_dir` (`<run_id>.md`, owner-only
/// permissions) and return the manifest. The directory is created
/// when missing; an existing report file is replaced.
pub(crate) fn write_report(
    detail: &RunDetail,
    terminal: Option<ExportTerminal<'_>>,
    out_dir: &Path,
) -> Result<WorkflowExportManifest> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating export directory {}", out_dir.display()))?;
    let report = render_report(detail, terminal);
    let path = out_dir.join(format!("{}.md", detail.summary.run_id));
    std::fs::write(&path, report.as_bytes())
        .with_context(|| format!("writing export report {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting export report {}", path.display()))?;
    }
    Ok(WorkflowExportManifest {
        run_id: detail.summary.run_id.clone(),
        report_path: path,
        included_terminal: terminal.is_some(),
        event_count: detail.events.len(),
        evidence_count: detail.evidence.len(),
        checkpoint_count: detail.checkpoints.len(),
    })
}

#[cfg(test)]
mod tests;
