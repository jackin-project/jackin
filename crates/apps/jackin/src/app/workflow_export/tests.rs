// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    use jackin_workflow::{CheckpointRecord, EvidenceRecord, SessionAttachment, WorkflowService};

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
