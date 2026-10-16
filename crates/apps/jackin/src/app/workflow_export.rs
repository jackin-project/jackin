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
mod tests {
    use jackin_workflow::{RunDetail, RunSummary, StoredCheckpoint, StoredEvidence};

    use super::{ExportTerminal, render_report};
    use jackin_workflow::spool::SessionSpool;

    fn fixture_detail() -> RunDetail {
        RunDetail {
            summary: RunSummary {
                run_id: "run-001".to_owned(),
                plan_hash: "plan-abc".to_owned(),
                status: "paused".to_owned(),
                stage: "test".to_owned(),
                round: 1,
                candidate: Some("cand-1".to_owned()),
                candidate_generation: 2,
                version: 3,
                owner_epoch: 7,
                pause_kind: Some("operator".to_owned()),
                max_repair_turns: 3,
            },
            sessions: Vec::new(),
            effects: Vec::new(),
            events: Vec::new(),
            evidence: vec![StoredEvidence {
                candidate: "cand-1".to_owned(),
                kind: "test".to_owned(),
                source: "host".to_owned(),
                quality: "observed".to_owned(),
                result: "fail".to_owned(),
                policy_hash: None,
                artifact_digest: None,
            }],
            checkpoints: vec![StoredCheckpoint {
                candidate: "cand-1".to_owned(),
                commit_hash: "deadbeef".to_owned(),
                tree_hash: "tree1".to_owned(),
                source_manifest: "manifest".to_owned(),
                local_path: "/tmp/work".to_owned(),
                backup_state: "local_only".to_owned(),
                preservation_hold: None,
            }],
        }
    }

    #[test]
    fn report_contains_evidence_and_no_terminal_by_default() {
        let detail = fixture_detail();
        let report = render_report(&detail, None);
        assert!(report.contains("run-001"));
        assert!(report.contains("cand-1"));
        assert!(report.contains("Terminal output excluded"));
    }

    #[test]
    fn report_with_terminal_marks_gaps_and_sanitizes() {
        let detail = fixture_detail();
        let mut spool = SessionSpool::new(8);
        spool.push("0123456789\x1b[31mred");
        let read = spool.read();
        let report = render_report(&detail, Some(ExportTerminal { read }));
        assert!(report.contains("Observation gap"));
        assert!(!report.contains('\u{1b}'));
    }

    /// K6 export proof against store fixtures (no live run): a
    /// paused run with test/review evidence and a checkpoint exports
    /// every section, and the file lands with owner-only
    /// permissions.
    #[test]
    fn report_from_store_fixtures_covers_evidence_and_checkpoints() {
        use jackin_workflow::{
            CheckpointRecord, EvidenceRecord, SessionAttachment, WorkflowService,
        };

        use super::write_report;

        let temp = tempfile::tempdir().expect("export fixture tempdir");
        let service =
            WorkflowService::open(&temp.path().join("runs.db"), &temp.path().join("artifacts"))
                .expect("open fixture service");
        let started = service
            .start_run("plan-export", 3, "key-export")
            .expect("start fixture run");
        service
            .store()
            .record_session(&SessionAttachment {
                run_id: &started.run_id,
                worker_id: "implementer",
                instance_id: Some("inst-export"),
                capsule_epoch: Some(4),
                session_id: Some(11),
                process_identity: Some("pid:11"),
                agent_profile: None,
                auth_profile: None,
            })
            .expect("record fixture session");
        for (kind, result) in [("test", "fail"), ("review", "pass")] {
            service
                .store()
                .record_evidence(&EvidenceRecord {
                    run_id: &started.run_id,
                    candidate: "cand-9",
                    kind,
                    source: "host",
                    quality: "observed",
                    result,
                    policy_hash: None,
                    artifact_digest: None,
                })
                .expect("record fixture evidence");
        }
        service
            .store()
            .record_checkpoint(&CheckpointRecord {
                run_id: &started.run_id,
                candidate: "cand-9",
                commit: "abc123",
                tree: "tree9",
                source_manifest: "manifest",
                local_path: "/tmp/fixture-work",
                backup_state: "backed_up",
                preservation_hold: Some("hold-1"),
            })
            .expect("record fixture checkpoint");
        service
            .pause_run(&started.run_id, "operator", "export proof", false)
            .expect("pause fixture run");

        let detail = service
            .run_detail(&started.run_id, -1, 200)
            .expect("fixture detail");
        let out_dir = temp.path().join("reports");
        let manifest = write_report(&detail, None, &out_dir).expect("write fixture report");
        assert_eq!(manifest.evidence_count, 2);
        assert_eq!(manifest.checkpoint_count, 1);
        assert!(!manifest.included_terminal);

        let text = std::fs::read_to_string(&manifest.report_path).expect("read fixture report");
        for section in [
            "## Sessions",
            "## Effects and backup state",
            "## Evidence (test and review)",
            "## Checkpoints and preserved work",
            "## Event history",
        ] {
            assert!(text.contains(section), "missing {section}");
        }
        assert!(text.contains("test cand-9"));
        assert!(text.contains("review cand-9"));
        assert!(text.contains("backup=backed_up"));
        assert!(text.contains("preservation hold: hold-1"));
        assert!(text.contains("- status: paused"));
        assert!(text.contains("- paused as: operator"));
        assert!(!text.contains('\u{1b}'));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&manifest.report_path)
                    .expect("report metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
