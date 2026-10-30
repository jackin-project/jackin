// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use super::super::{marker_pair::MarkerPairLease, marker_transaction};
use super::case_03::{
    self, MarkerPairChild, MarkerPairTestRoot, MarkerPairTransactionChildCommand,
};

const OLD_SELECTED: &[u8] = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OLD_ADMISSION: &[u8] = b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NEW_SELECTED: &[u8] = b"1111111111111111111111111111111111111111111111111111111111111111";
const NEW_ADMISSION: &[u8] = b"2222222222222222222222222222222222222222222222222222222222222222";
const TEST_ROLE: &str = "marker-pair-test-role";

#[derive(Clone, Debug, Eq, PartialEq)]
struct EntrySnapshot {
    kind: &'static str,
    mode: u32,
    owner: u32,
    size: u64,
    device: u64,
    inode: u64,
    links: u64,
    bytes: Option<Vec<u8>>,
    link_target: Option<PathBuf>,
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, EntrySnapshot> {
    let root_metadata = std::fs::symlink_metadata(root).unwrap();
    let mut entries = BTreeMap::from([(
        PathBuf::new(),
        EntrySnapshot {
            kind: "directory",
            mode: root_metadata.mode(),
            owner: root_metadata.uid(),
            size: root_metadata.size(),
            device: root_metadata.dev(),
            inode: root_metadata.ino(),
            links: root_metadata.nlink(),
            bytes: None,
            link_target: None,
        },
    )]);
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let file_type = metadata.file_type();
        let kind = if file_type.is_symlink() {
            "symlink"
        } else if file_type.is_file() {
            "file"
        } else if file_type.is_dir() {
            "directory"
        } else {
            "other"
        };
        entries.insert(
            entry.file_name().into(),
            EntrySnapshot {
                kind,
                mode: metadata.mode(),
                owner: metadata.uid(),
                size: metadata.size(),
                device: metadata.dev(),
                inode: metadata.ino(),
                links: metadata.nlink(),
                bytes: file_type.is_file().then(|| std::fs::read(&path).unwrap()),
                link_target: file_type
                    .is_symlink()
                    .then(|| std::fs::read_link(&path).unwrap()),
            },
        );
    }
    entries
}

fn publish_pair(root: &Path, selected: &[u8], admission: &[u8]) {
    let lease = MarkerPairLease::acquire(root).unwrap();
    marker_transaction::publish_locked(&lease, selected, admission, || Ok(())).unwrap();
}

fn recover(root: &Path) {
    let lease = marker_transaction::shared_lease_with_recovery(root)
        .unwrap()
        .expect("fixture root remains present");
    drop(lease);
}

fn recover_result(root: &Path) -> anyhow::Result<()> {
    let lease = marker_transaction::shared_lease_with_recovery(root)?
        .expect("fixture root remains present");
    drop(lease);
    Ok(())
}

fn publish_result(root: &Path, selected: &[u8], admission: &[u8]) -> anyhow::Result<()> {
    let lease = MarkerPairLease::acquire(root)?;
    marker_transaction::publish_locked(&lease, selected, admission, || Ok(()))
}

fn assert_pair(root: &Path, selected: &[u8], admission: &[u8]) {
    assert_eq!(
        std::fs::read(root.join("account-config.sha256")).unwrap(),
        selected
    );
    assert_eq!(
        std::fs::read(root.join("account-admission.sha256")).unwrap(),
        admission
    );
    assert!(!root.join(".account-markers-transaction").exists());
    assert!(!root.join(".account-markers-transaction.next").exists());
    let residues: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".account-markers-"))
        .collect();
    assert!(
        residues.is_empty(),
        "marker transaction residues remain: {residues:?}"
    );
}

fn transaction_names(root: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".account-markers-"))
        .collect();
    names.sort();
    names
}

fn pause_publication(fixture: &MarkerPairTestRoot, command: MarkerPairTransactionChildCommand) {
    let (mut child, child_id) =
        MarkerPairChild::spawn_transaction(fixture.path(), fixture.preservation_flag(), command)
            .unwrap();
    assert_ne!(child_id, std::process::id());
    child.terminate_and_reap().unwrap();
}

fn overwrite_file(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

fn set_mode(path: &Path, mode: u32) {
    let mut permissions = std::fs::symlink_metadata(path).unwrap().permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn expect_publication_failure(
    result: anyhow::Result<()>,
    expected: marker_transaction::PublicationFailureKind,
) -> anyhow::Error {
    let Err(error) = result else {
        panic!("expected marker publication failure {expected:?}");
    };
    assert_eq!(
        marker_transaction::publication_failure_kind(&error),
        Some(expected)
    );
    error
}

fn expect_error(result: anyhow::Result<()>, message: &str) -> anyhow::Error {
    let Err(error) = result else {
        panic!("expected error: {message}");
    };
    error
}

fn pause_prepared(fixture: &MarkerPairTestRoot) {
    pause_publication(
        fixture,
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    );
}

fn handoff_root_paths(root: &Path) -> (PathBuf, PathBuf) {
    let name = root.file_name().unwrap().to_string_lossy();
    let original = root.with_file_name(format!("{name}-original"));
    let replacement = root.with_file_name(format!("{name}-replacement"));
    (original, replacement)
}

fn replace_root_for_handoff(root: &Path, original: &Path, replacement: &Path) {
    assert!(!original.exists());
    assert!(!replacement.exists());
    std::fs::rename(root, original).unwrap();
    std::fs::create_dir(root).unwrap();
    let mut permissions = std::fs::symlink_metadata(root).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(root, permissions).unwrap();
    std::fs::write(root.join("replacement-sentinel"), b"replacement root").unwrap();
}

fn restore_root_after_handoff(root: &Path, original: &Path, replacement: &Path) {
    std::fs::rename(root, replacement).unwrap();
    std::fs::rename(original, root).unwrap();
    std::fs::remove_dir_all(replacement).unwrap();
}

#[test]
fn candidate_and_prepared_write_failures_preserve_or_recover_the_old_pair() {
    use marker_transaction::TransactionFaultPoint as Fault;

    for fault in [
        Fault::CandidateWrite,
        Fault::CandidateFileSync,
        Fault::CandidateRootSync,
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        let error = {
            let _faults = marker_transaction::inject_faults_for_test(&[fault]);
            expect_publication_failure(
                publish_result(root, NEW_SELECTED, NEW_ADMISSION),
                marker_transaction::PublicationFailureKind::PreCommitFailure,
            )
        };
        assert!(format!("{error:#}").contains("injected marker transaction failure"));
        assert_eq!(
            std::fs::read(root.join("account-config.sha256")).unwrap(),
            OLD_SELECTED
        );
        assert_eq!(
            std::fs::read(root.join("account-admission.sha256")).unwrap(),
            OLD_ADMISSION
        );
        assert!(!root.join(".account-markers-transaction").exists());
        let names = transaction_names(root);
        assert_eq!(names.len(), 3);
        assert!(names.iter().any(|name| name.ends_with("-selected")));
        assert!(names.iter().any(|name| name.ends_with("-admission")));
        let candidate = root.join(".account-markers-transaction.next");
        let candidate_metadata = std::fs::symlink_metadata(&candidate).unwrap();
        assert_eq!(candidate_metadata.permissions().mode() & 0o777, 0o600);
        if fault == Fault::CandidateWrite {
            let bytes = std::fs::read(&candidate).unwrap();
            assert_eq!(bytes.len(), 32);
        } else {
            assert!(std::fs::read(&candidate).unwrap().len() > 32);
        }
        let before_refusal = snapshot(root);
        let error = expect_error(recover_result(root), "orphan candidate refusal");
        drop(error);
        assert_eq!(snapshot(root), before_refusal);
    }

    for fault in [
        Fault::PreparedWrite,
        Fault::PreparedFileSync,
        Fault::PreparedRootSync,
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        let error = {
            let _faults = marker_transaction::inject_faults_for_test(&[fault]);
            expect_publication_failure(
                publish_result(root, NEW_SELECTED, NEW_ADMISSION),
                marker_transaction::PublicationFailureKind::PreCommitFailure,
            )
        };
        assert!(format!("{error:#}").contains("injected marker transaction failure"));
        assert_eq!(
            std::fs::read(root.join("account-config.sha256")).unwrap(),
            OLD_SELECTED
        );
        assert_eq!(
            std::fs::read(root.join("account-admission.sha256")).unwrap(),
            OLD_ADMISSION
        );
        assert!(root.join(".account-markers-transaction").exists());
        assert!(root.join(".account-markers-transaction.next").exists());
        assert_eq!(transaction_names(root).len(), 4);
        if fault == Fault::PreparedWrite {
            let bytes = std::fs::read(root.join(".account-markers-transaction")).unwrap();
            assert_eq!(bytes.len(), 32);
            let before_refusal = snapshot(root);
            let error = expect_error(recover_result(root), "partial Prepared record refusal");
            drop(error);
            assert_eq!(snapshot(root), before_refusal);
        } else {
            recover(root);
            assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
        }
    }
}

#[test]
fn prepared_install_preflights_the_journal_candidate_and_both_old_inputs() {
    #[derive(Clone, Copy)]
    enum Tamper {
        Prepared,
        Candidate,
        Selected,
        Admission,
        SelectedStage,
        AdmissionStage,
    }

    for tamper in [
        Tamper::Prepared,
        Tamper::Candidate,
        Tamper::Selected,
        Tamper::Admission,
        Tamper::SelectedStage,
        Tamper::AdmissionStage,
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        let (child, child_id) = MarkerPairChild::spawn_transaction(
            root,
            fixture.preservation_flag(),
            MarkerPairTransactionChildCommand::PauseAfterPreparedExpectingPreflightRefusal,
        )
        .unwrap();
        assert_ne!(child_id, std::process::id());

        match tamper {
            Tamper::Prepared => {
                let path = root.join(".account-markers-transaction");
                let mut bytes = std::fs::read(&path).unwrap();
                let field = b"\"transaction_id\":\"";
                let start = bytes
                    .windows(field.len())
                    .position(|window| window == field)
                    .unwrap()
                    + field.len();
                bytes[start] = if bytes[start] == b'0' { b'1' } else { b'0' };
                overwrite_file(&path, &bytes);
            }
            Tamper::Candidate => {
                let path = root.join(".account-markers-transaction.next");
                let mut bytes = std::fs::read(&path).unwrap();
                bytes[0] ^= 1;
                overwrite_file(&path, &bytes);
            }
            Tamper::Selected => overwrite_file(
                &root.join("account-config.sha256"),
                b"changed selected input",
            ),
            Tamper::Admission => overwrite_file(
                &root.join("account-admission.sha256"),
                b"changed admission input",
            ),
            Tamper::SelectedStage | Tamper::AdmissionStage => {
                let suffix = if matches!(tamper, Tamper::SelectedStage) {
                    "-selected"
                } else {
                    "-admission"
                };
                let stage = std::fs::read_dir(root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .ends_with(suffix)
                    })
                    .unwrap();
                overwrite_file(&stage, b"changed staged input");
            }
        }
        let before_release = snapshot(root);
        child.release().unwrap();
        assert_eq!(snapshot(root), before_release);
        assert!(root.join(".account-markers-transaction").exists());
    }
}

#[test]
fn selected_and_admission_rename_failures_roll_back_before_commit() {
    use marker_transaction::TransactionFaultPoint as Fault;

    for fault in [
        Fault::SelectedRename,
        Fault::SelectedRootSync,
        Fault::AdmissionRename,
        Fault::AdmissionRootSync,
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        {
            let _faults = marker_transaction::inject_faults_for_test(&[fault]);
            expect_publication_failure(
                publish_result(root, NEW_SELECTED, NEW_ADMISSION),
                marker_transaction::PublicationFailureKind::PreCommitFailure,
            );
        }
        assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
    }
}

#[test]
fn commit_failures_remain_uncertain_and_never_roll_back_installed_values() {
    use marker_transaction::TransactionFaultPoint as Fault;

    for (fault, expected_phase, candidate_remains, recovered_selected, recovered_admission) in [
        (
            Fault::CommitRenameBefore,
            "prepared",
            true,
            OLD_SELECTED,
            OLD_ADMISSION,
        ),
        (
            Fault::CommitRename,
            "committed",
            false,
            NEW_SELECTED,
            NEW_ADMISSION,
        ),
        (
            Fault::CommitRootSync,
            "committed",
            false,
            NEW_SELECTED,
            NEW_ADMISSION,
        ),
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        {
            let _faults = marker_transaction::inject_faults_for_test(&[fault]);
            expect_publication_failure(
                publish_result(root, NEW_SELECTED, NEW_ADMISSION),
                marker_transaction::PublicationFailureKind::CommitUncertain,
            );
        }
        assert_eq!(
            std::fs::read(root.join("account-config.sha256")).unwrap(),
            NEW_SELECTED
        );
        assert_eq!(
            std::fs::read(root.join("account-admission.sha256")).unwrap(),
            NEW_ADMISSION
        );
        let primary = std::fs::read_to_string(root.join(".account-markers-transaction")).unwrap();
        let phase_marker = format!("\"phase\":\"{expected_phase}\"");
        assert!(primary.contains(phase_marker.as_str()));
        assert_eq!(
            root.join(".account-markers-transaction.next").exists(),
            candidate_remains
        );
        if candidate_remains {
            let candidate =
                std::fs::read_to_string(root.join(".account-markers-transaction.next")).unwrap();
            assert!(candidate.contains("\"phase\":\"committed\""));
        }
        recover(root);
        assert_pair(root, recovered_selected, recovered_admission);
    }
}

#[test]
fn committed_cleanup_failures_keep_the_new_pair_authoritative() {
    use marker_transaction::TransactionFaultPoint as Fault;

    for (fault, journal_remains) in [
        (Fault::CommittedPrimaryUnlink, true),
        (Fault::CommittedFinalRootSync, false),
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        {
            let _faults = marker_transaction::inject_faults_for_test(&[fault]);
            expect_publication_failure(
                publish_result(root, NEW_SELECTED, NEW_ADMISSION),
                marker_transaction::PublicationFailureKind::CommittedCleanupPending,
            );
        }
        assert_eq!(
            std::fs::read(root.join("account-config.sha256")).unwrap(),
            NEW_SELECTED
        );
        assert_eq!(
            std::fs::read(root.join("account-admission.sha256")).unwrap(),
            NEW_ADMISSION
        );
        assert_eq!(
            root.join(".account-markers-transaction").exists(),
            journal_remains
        );
        assert!(!root.join(".account-markers-transaction.next").exists());
        recover(root);
        assert_pair(root, NEW_SELECTED, NEW_ADMISSION);
    }
}

#[test]
fn prepared_rollback_residue_cleanup_is_idempotent_at_each_boundary() {
    use marker_transaction::TransactionFaultPoint as Fault;

    for (fault, journal_remains, candidate_remains) in [
        (Fault::PreparedStageUnlink, true, true),
        (Fault::PreparedCandidateUnlink, true, true),
        (Fault::PreparedResidueRootSync, true, false),
        (Fault::PreparedPrimaryUnlink, true, false),
        (Fault::PreparedFinalRootSync, false, false),
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        pause_prepared(&fixture);
        let error = {
            let _faults = marker_transaction::inject_faults_for_test(&[fault]);
            recover_result(root).expect_err("injected Prepared cleanup boundary must fail")
        };
        assert!(format!("{error:#}").contains("injected marker transaction failure"));
        assert_eq!(
            std::fs::read(root.join("account-config.sha256")).unwrap(),
            OLD_SELECTED
        );
        assert_eq!(
            std::fs::read(root.join("account-admission.sha256")).unwrap(),
            OLD_ADMISSION
        );
        assert_eq!(
            root.join(".account-markers-transaction").exists(),
            journal_remains
        );
        assert_eq!(
            root.join(".account-markers-transaction.next").exists(),
            candidate_remains
        );
        recover(root);
        assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
    }
}

#[test]
fn rollback_failure_preserves_both_publication_and_partial_restore_causes() {
    use marker_transaction::TransactionFaultPoint as Fault;
    use marker_transaction::TransactionRole as Role;

    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    let error = {
        let _faults = marker_transaction::inject_faults_for_test(&[
            Fault::AdmissionRootSync,
            Fault::RollbackPartialWrite(Role::Selected),
        ]);
        expect_publication_failure(
            publish_result(root, NEW_SELECTED, NEW_ADMISSION),
            marker_transaction::PublicationFailureKind::PreparedRollbackPending,
        )
    };
    let message = format!("{error:#}");
    assert!(message.contains("AdmissionRootSync"));
    assert!(message.contains("partial old-value write"));
    assert!(
        std::fs::read(root.join("account-config.sha256"))
            .unwrap()
            .len()
            < OLD_SELECTED.len()
    );
    assert_eq!(
        std::fs::read(root.join("account-admission.sha256")).unwrap(),
        NEW_ADMISSION
    );
    recover(root);
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
}

#[test]
fn rollback_write_truncate_and_mode_sync_failures_resume_by_role() {
    use marker_transaction::TransactionFaultPoint as Fault;
    use marker_transaction::TransactionRole as Role;

    for (role, pause) in [
        (
            Role::Selected,
            MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
        ),
        (
            Role::Admission,
            MarkerPairTransactionChildCommand::PauseAfterAdmissionInstall,
        ),
    ] {
        for partial in [false, true] {
            let fixture = MarkerPairTestRoot::new().unwrap();
            let root = fixture.path();
            publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
            pause_publication(&fixture, pause);
            let fault = if partial {
                Fault::RollbackPartialWrite(role)
            } else {
                Fault::RollbackTruncate(role)
            };
            {
                let _faults = marker_transaction::inject_faults_for_test(&[fault]);
                let error = expect_error(recover_result(root), "injected rollback write boundary");
                assert!(format!("{error:#}").contains("injected marker transaction failure"));
            }
            let target = match role {
                Role::Selected => "account-config.sha256",
                Role::Admission => "account-admission.sha256",
            };
            let observed = std::fs::read(root.join(target)).unwrap();
            if partial {
                assert!(observed.len() < OLD_SELECTED.len());
            } else {
                assert_eq!(observed.len(), 64);
            }
            recover(root);
            assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
        }

        for fault_kind in [0, 1, 2] {
            let fixture = MarkerPairTestRoot::new().unwrap();
            let root = fixture.path();
            publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
            for name in ["account-config.sha256", "account-admission.sha256"] {
                set_mode(&root.join(name), 0o444);
            }
            pause_publication(
                &fixture,
                MarkerPairTransactionChildCommand::PauseAfterAdmissionInstall,
            );
            let (target, old) = match role {
                Role::Selected => ("account-config.sha256", OLD_SELECTED),
                Role::Admission => ("account-admission.sha256", OLD_ADMISSION),
            };
            overwrite_file(&root.join(target), old);
            let fault = match fault_kind {
                0 => Fault::RollbackModeRestore(role),
                1 => Fault::RollbackFileSync(role),
                _ => Fault::RollbackRootSync(role),
            };
            {
                let _faults = marker_transaction::inject_faults_for_test(&[fault]);
                let error = expect_error(
                    recover_result(root),
                    "injected rollback mode or sync boundary",
                );
                assert!(format!("{error:#}").contains("injected marker transaction failure"));
            }
            let metadata = std::fs::symlink_metadata(root.join(target)).unwrap();
            assert_eq!(std::fs::read(root.join(target)).unwrap(), old);
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                if fault_kind == 0 { 0o600 } else { 0o444 }
            );
            recover(root);
            assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
            for name in ["account-config.sha256", "account-admission.sha256"] {
                assert_eq!(
                    std::fs::symlink_metadata(root.join(name))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o444
                );
            }
        }
    }
}

#[test]
fn recovery_refuses_a_replaced_root_at_both_lock_handoff_boundaries() {
    use marker_transaction::RootHandoffPoint as Handoff;

    for point in [Handoff::AfterSharedRelease, Handoff::AfterExclusiveRelease] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        pause_prepared(&fixture);
        let (original, replacement) = handoff_root_paths(root);
        let hook = marker_transaction::pause_root_handoff_for_test(point, move |path| {
            replace_root_for_handoff(path, &original, &replacement);
            Ok(())
        });
        let result = recover_result(root);
        drop(hook);
        let error = expect_error(result, "root identity change during lock handoff");
        drop(error);
        assert_eq!(
            std::fs::read(root.join("replacement-sentinel")).unwrap(),
            b"replacement root"
        );
        let (original, replacement) = handoff_root_paths(root);
        let old_namespace = snapshot(&original);
        if point == Handoff::AfterSharedRelease {
            assert!(old_namespace.contains_key(Path::new(".account-markers-transaction")));
            assert!(old_namespace.contains_key(Path::new(".account-markers-transaction.next")));
        } else {
            assert!(!old_namespace.contains_key(Path::new(".account-markers-transaction")));
            assert!(!old_namespace.contains_key(Path::new(".account-markers-transaction.next")));
        }
        restore_root_after_handoff(root, &original, &replacement);
        if point == Handoff::AfterSharedRelease {
            recover(root);
        }
        assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
    }
}

#[test]
fn shared_recovery_refuses_busy_exclusive_handoff_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_prepared(&fixture);
    let held_shared = MarkerPairLease::acquire_optional(root).unwrap().unwrap();
    let before = snapshot(root);
    let Err(error) = recover_result(root) else {
        panic!("recovery must not wait for a busy exclusive handoff");
    };
    assert!(format!("{error:#}").contains("busy"));
    assert_eq!(snapshot(root), before);
    drop(held_shared);
    recover(root);
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
}

#[test]
fn newer_writer_after_recovery_handoff_is_not_overwritten_by_the_reader() {
    use marker_transaction::RootHandoffPoint as Handoff;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_prepared(&fixture);

    let (reached_sender, reached) = channel();
    let (resume_sender, resume) = channel();
    let child_root = root.to_path_buf();
    let reader = std::thread::spawn(move || {
        let _hook = marker_transaction::pause_root_handoff_for_test(
            Handoff::AfterExclusiveRelease,
            move |_| {
                reached_sender.send(()).unwrap();
                resume
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(anyhow::Error::from)
            },
        );
        recover_result(&child_root)
    });
    reached.recv_timeout(Duration::from_secs(10)).unwrap();
    publish_pair(root, NEW_SELECTED, NEW_ADMISSION);
    resume_sender.send(()).unwrap();
    reader.join().unwrap().unwrap();
    assert_pair(root, NEW_SELECTED, NEW_ADMISSION);
}

#[test]
fn prepared_restart_after_first_install_restores_old_pair() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    let before_selected = std::fs::symlink_metadata(root.join("account-config.sha256")).unwrap();
    let before_admission =
        std::fs::symlink_metadata(root.join("account-admission.sha256")).unwrap();

    let (mut child, child_id) = MarkerPairChild::spawn_transaction(
        root,
        fixture.preservation_flag(),
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    )
    .unwrap();
    assert_ne!(child_id, std::process::id());
    let during = snapshot(root);
    assert_eq!(
        during
            .get(Path::new("account-config.sha256"))
            .unwrap()
            .bytes
            .as_deref(),
        Some(NEW_SELECTED)
    );
    assert_eq!(
        during
            .get(Path::new("account-admission.sha256"))
            .unwrap()
            .bytes
            .as_deref(),
        Some(OLD_ADMISSION)
    );
    child.terminate_and_reap().unwrap();
    drop(child);

    recover(root);
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
    recover(root);
    let restored_selected = std::fs::symlink_metadata(root.join("account-config.sha256")).unwrap();
    let restored_admission =
        std::fs::symlink_metadata(root.join("account-admission.sha256")).unwrap();
    assert_ne!(
        (restored_selected.dev(), restored_selected.ino()),
        (before_selected.dev(), before_selected.ino())
    );
    assert_eq!(
        (restored_admission.dev(), restored_admission.ino()),
        (before_admission.dev(), before_admission.ino())
    );
    assert_eq!(
        restored_admission.permissions().mode() & 0o777,
        before_admission.permissions().mode() & 0o777
    );
}

#[test]
fn prepared_prefix_recovery_reopens_zero_and_non64_byte_old_values() {
    for (old_selected, prefix_len) in [(vec![b's'; 19], 0), (vec![b'l'; 90], 19)] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        let selected = root.join("account-config.sha256");
        overwrite_file(&selected, &old_selected);
        let old_mode = std::fs::symlink_metadata(&selected)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        pause_publication(
            &fixture,
            MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
        );
        overwrite_file(&selected, &old_selected[..prefix_len]);
        let interrupted = std::fs::symlink_metadata(&selected).unwrap();

        recover(root);

        let restored = std::fs::symlink_metadata(&selected).unwrap();
        assert_eq!(std::fs::read(&selected).unwrap(), old_selected);
        assert_eq!(
            (restored.dev(), restored.ino()),
            (interrupted.dev(), interrupted.ino())
        );
        assert_eq!(restored.permissions().mode() & 0o777, old_mode);
        assert_eq!(
            std::fs::read(root.join("account-admission.sha256")).unwrap(),
            OLD_ADMISSION
        );
        assert_pair(root, &old_selected, OLD_ADMISSION);
    }
}

#[test]
fn fresh_owned_child_recovers_a_recognized_prepared_transaction() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    let (child, child_id) = MarkerPairChild::spawn_transaction(
        root,
        fixture.preservation_flag(),
        MarkerPairTransactionChildCommand::RecoverInFreshChild,
    )
    .unwrap();
    assert_ne!(child_id, std::process::id());
    child.release().unwrap();
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
}

#[test]
fn public_matchers_recover_a_recognized_prepared_transaction() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    let (config, admitted) = admitted_fingerprint_fixture();
    let paths = case_03::prepare_marker_root(root, &config, &admitted);
    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    case_03::record_pair(root, &paths, &revision, &config, &config, &admitted).unwrap();
    drop(revision);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    assert!(account_configuration_matches(root, &config, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(root, &config, None, TEST_ROLE).unwrap());
    assert!(!root.join(".account-markers-transaction").exists());
    assert!(!root.join(".account-markers-transaction.next").exists());
}

#[test]
fn public_publisher_recovers_before_publishing_a_new_pair() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    let (config, admitted) = admitted_fingerprint_fixture();
    let paths = case_03::prepare_marker_root(root, &config, &admitted);
    let initial_revision = AccountConfigRevision::acquire(&paths).unwrap();
    case_03::record_pair(root, &paths, &initial_revision, &config, &config, &admitted).unwrap();
    drop(initial_revision);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    case_03::record_pair(root, &paths, &revision, &config, &config, &admitted).unwrap();
    assert!(account_configuration_matches(root, &config, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(root, &config, None, TEST_ROLE).unwrap());
    assert!(!root.join(".account-markers-transaction").exists());
    assert!(!root.join(".account-markers-transaction.next").exists());
}

#[test]
fn prepared_restart_at_remaining_publication_boundaries_restores_old_pair() {
    for command in [
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
        MarkerPairTransactionChildCommand::PauseAfterAdmissionInstall,
        MarkerPairTransactionChildCommand::PauseBeforeCommitRename,
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        let (mut child, child_id) =
            MarkerPairChild::spawn_transaction(root, fixture.preservation_flag(), command).unwrap();
        assert_ne!(child_id, std::process::id());
        child.terminate_and_reap().unwrap();
        drop(child);

        recover(root);
        assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
    }
}

#[test]
fn commit_rename_restart_keeps_new_pair_and_cleans_journal() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);

    let (mut child, child_id) = MarkerPairChild::spawn_transaction(
        root,
        fixture.preservation_flag(),
        MarkerPairTransactionChildCommand::PauseAfterCommitRename,
    )
    .unwrap();
    assert_ne!(child_id, std::process::id());
    let during = snapshot(root);
    assert_eq!(
        during
            .get(Path::new("account-config.sha256"))
            .unwrap()
            .bytes
            .as_deref(),
        Some(NEW_SELECTED)
    );
    assert_eq!(
        during
            .get(Path::new("account-admission.sha256"))
            .unwrap()
            .bytes
            .as_deref(),
        Some(NEW_ADMISSION)
    );
    assert!(during.contains_key(Path::new(".account-markers-transaction")));
    child.terminate_and_reap().unwrap();
    drop(child);

    recover(root);
    assert_pair(root, NEW_SELECTED, NEW_ADMISSION);
}

#[test]
fn committed_restart_after_directory_sync_keeps_new_pair_and_cleans_journal() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);

    let (mut child, child_id) = MarkerPairChild::spawn_transaction(
        root,
        fixture.preservation_flag(),
        MarkerPairTransactionChildCommand::PauseAfterCommitSync,
    )
    .unwrap();
    assert_ne!(child_id, std::process::id());
    let during = snapshot(root);
    assert_eq!(
        during
            .get(Path::new("account-config.sha256"))
            .unwrap()
            .bytes
            .as_deref(),
        Some(NEW_SELECTED)
    );
    assert_eq!(
        during
            .get(Path::new("account-admission.sha256"))
            .unwrap()
            .bytes
            .as_deref(),
        Some(NEW_ADMISSION)
    );
    assert!(during.contains_key(Path::new(".account-markers-transaction")));
    child.terminate_and_reap().unwrap();
    drop(child);

    recover(root);
    assert_pair(root, NEW_SELECTED, NEW_ADMISSION);
}

#[test]
fn committed_cleanup_refusal_preserves_synced_new_pair_and_journal() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterCommitSync,
    );
    set_mode(root, 0o500);
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected committed cleanup refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);

    set_mode(root, 0o700);
    recover(root);
    assert_pair(root, NEW_SELECTED, NEW_ADMISSION);
}

#[test]
fn prepared_restart_preserves_read_only_old_modes() {
    for old_mode in [0o400, 0o444] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        for name in ["account-config.sha256", "account-admission.sha256"] {
            let path = root.join(name);
            let mut permissions = std::fs::symlink_metadata(&path).unwrap().permissions();
            permissions.set_mode(old_mode);
            std::fs::set_permissions(&path, permissions).unwrap();
        }

        let (mut child, _) = MarkerPairChild::spawn_transaction(
            root,
            fixture.preservation_flag(),
            MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
        )
        .unwrap();
        child.terminate_and_reap().unwrap();
        drop(child);

        recover(root);
        assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
        for name in ["account-config.sha256", "account-admission.sha256"] {
            assert_eq!(
                std::fs::symlink_metadata(root.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777,
                old_mode
            );
        }
    }
}

#[test]
fn prepared_restart_without_prior_markers_restores_absence() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    let (mut child, _) = MarkerPairChild::spawn_transaction(
        root,
        fixture.preservation_flag(),
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    )
    .unwrap();
    child.terminate_and_reap().unwrap();
    drop(child);

    recover(root);
    assert!(!root.join("account-config.sha256").exists());
    assert!(!root.join("account-admission.sha256").exists());
    assert!(!root.join(".account-markers-transaction").exists());
    assert!(!root.join(".account-markers-transaction.next").exists());
}

#[test]
fn prepared_restart_restores_each_single_absent_marker_state() {
    for (absent_name, pause) in [
        (
            "account-config.sha256",
            MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
        ),
        (
            "account-admission.sha256",
            MarkerPairTransactionChildCommand::PauseAfterAdmissionInstall,
        ),
    ] {
        let fixture = MarkerPairTestRoot::new().unwrap();
        let root = fixture.path();
        publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
        std::fs::remove_file(root.join(absent_name)).unwrap();
        let (mut child, child_id) =
            MarkerPairChild::spawn_transaction(root, fixture.preservation_flag(), pause).unwrap();
        assert_ne!(child_id, std::process::id());
        child.terminate_and_reap().unwrap();
        drop(child);

        recover(root);
        assert!(!root.join(absent_name).exists());
        if absent_name == "account-config.sha256" {
            assert_eq!(
                std::fs::read(root.join("account-admission.sha256")).unwrap(),
                OLD_ADMISSION
            );
        } else {
            assert_eq!(
                std::fs::read(root.join("account-config.sha256")).unwrap(),
                OLD_SELECTED
            );
        }
        assert!(!root.join(".account-markers-transaction").exists());
        assert!(!root.join(".account-markers-transaction.next").exists());
    }
}

#[test]
fn orphan_candidate_refusal_preserves_complete_root_snapshot() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    std::fs::write(
        root.join(".account-markers-transaction.next"),
        b"unbound candidate",
    )
    .unwrap();
    let before = snapshot(root);
    let error = marker_transaction::shared_lease_with_recovery(root)
        .err()
        .expect("orphan candidate must be refused");
    assert!(
        error
            .to_string()
            .contains("orphan marker transaction candidate")
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn truncated_primary_record_refusal_preserves_complete_root_snapshot() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    let journal = root.join(".account-markers-transaction");
    std::fs::write(&journal, br#"{"phase":"prepared","schema_version":1"#).unwrap();
    let mut permissions = std::fs::symlink_metadata(&journal).unwrap().permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(&journal, permissions).unwrap();
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected truncated journal refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn unsupported_schema_refusal_preserves_complete_root_snapshot() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    let (mut child, _) = MarkerPairChild::spawn_transaction(
        root,
        fixture.preservation_flag(),
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    )
    .unwrap();
    child.terminate_and_reap().unwrap();
    drop(child);

    let journal = root.join(".account-markers-transaction");
    let encoded = String::from_utf8(std::fs::read(&journal).unwrap()).unwrap();
    assert!(encoded.contains("\"schema_version\":2"));
    std::fs::write(
        &journal,
        encoded.replacen("\"schema_version\":2", "\"schema_version\":1", 1),
    )
    .unwrap();
    let before = snapshot(root);

    let error = marker_transaction::shared_lease_with_recovery(root)
        .err()
        .expect("legacy schema must be refused");
    assert!(
        error
            .to_string()
            .contains("unsupported marker schema version")
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_restart_completes_a_strict_old_value_prefix() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    overwrite_file(&root.join("account-config.sha256"), &OLD_SELECTED[..19]);
    recover(root);
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
}

#[test]
fn prepared_refuses_old_prefix_with_original_mode_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    for name in ["account-config.sha256", "account-admission.sha256"] {
        set_mode(&root.join(name), 0o444);
    }
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    let selected = root.join("account-config.sha256");
    overwrite_file(&selected, &OLD_SELECTED[..19]);
    set_mode(&selected, 0o444);
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected old-prefix mode refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_restart_finishes_complete_old_bytes_before_mode_restore() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    for name in ["account-config.sha256", "account-admission.sha256"] {
        set_mode(&root.join(name), 0o444);
    }
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    let selected = root.join("account-config.sha256");
    overwrite_file(&selected, OLD_SELECTED);
    let before = std::fs::symlink_metadata(&selected).unwrap();
    assert_eq!(before.permissions().mode() & 0o7777, 0o600);
    recover(root);
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
    let restored = std::fs::symlink_metadata(&selected).unwrap();
    assert_eq!(restored.permissions().mode() & 0o7777, 0o444);
    assert_eq!(
        (restored.dev(), restored.ino()),
        (before.dev(), before.ino())
    );
}

#[test]
fn prepared_restart_accepts_complete_old_value_at_original_mode_read_only() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    for name in ["account-config.sha256", "account-admission.sha256"] {
        set_mode(&root.join(name), 0o444);
    }
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    let selected = root.join("account-config.sha256");
    overwrite_file(&selected, OLD_SELECTED);
    set_mode(&selected, 0o444);
    let before = std::fs::symlink_metadata(&selected).unwrap();
    recover(root);
    let after = std::fs::symlink_metadata(&selected).unwrap();
    assert_eq!(
        (after.dev(), after.ino(), after.mode()),
        (before.dev(), before.ino(), before.mode())
    );
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
}

#[test]
fn prepared_refuses_candidate_mismatch_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    );

    let candidate = root.join(".account-markers-transaction.next");
    let size = std::fs::read(&candidate).unwrap().len();
    overwrite_file(&candidate, &vec![b'x'; size]);
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected candidate mismatch refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_refuses_missing_candidate_while_a_marker_needs_restore() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );
    std::fs::remove_file(root.join(".account-markers-transaction.next")).unwrap();
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected missing candidate refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_cleanup_accepts_missing_candidate_after_both_old_states_are_complete() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    );
    std::fs::remove_file(root.join(".account-markers-transaction.next")).unwrap();

    recover(root);
    assert_pair(root, OLD_SELECTED, OLD_ADMISSION);
}

#[test]
fn prepared_refuses_foreign_marker_inode_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterSelectedInstall,
    );

    let admission = root.join("account-admission.sha256");
    let old = std::fs::symlink_metadata(&admission).unwrap();
    let replacement = root.join("replacement-marker");
    std::fs::write(&replacement, OLD_ADMISSION).unwrap();
    std::fs::rename(&replacement, &admission).unwrap();
    let new = std::fs::symlink_metadata(&admission).unwrap();
    assert_ne!((old.dev(), old.ino()), (new.dev(), new.ino()));
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected foreign marker inode refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn committed_refuses_changed_marker_value_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterCommitRename,
    );

    overwrite_file(
        &root.join("account-admission.sha256"),
        b"3333333333333333333333333333333333333333333333333333333333333333",
    );
    let before = snapshot(root);
    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected committed marker mismatch refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_refuses_unknown_journal_field_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    );

    let journal = root.join(".account-markers-transaction");
    let original = std::fs::read(&journal).unwrap();
    let mut mutated = br#"{"unexpected":true,"#.to_vec();
    mutated.extend_from_slice(&original[1..]);
    overwrite_file(&journal, &mutated);
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected unknown journal field refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_refuses_duplicate_journal_field_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    );

    let journal = root.join(".account-markers-transaction");
    let original = std::fs::read(&journal).unwrap();
    let mut mutated = br#"{"schema_version":2,"#.to_vec();
    mutated.extend_from_slice(&original[1..]);
    overwrite_file(&journal, &mutated);
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected duplicate journal field refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}

#[test]
fn prepared_refuses_hardlinked_primary_without_mutation() {
    let fixture = MarkerPairTestRoot::new().unwrap();
    let root = fixture.path();
    publish_pair(root, OLD_SELECTED, OLD_ADMISSION);
    pause_publication(
        &fixture,
        MarkerPairTransactionChildCommand::PauseAfterPrepared,
    );
    std::fs::hard_link(
        root.join(".account-markers-transaction"),
        root.join("journal-alias"),
    )
    .unwrap();
    let before = snapshot(root);

    let Err(error) = marker_transaction::shared_lease_with_recovery(root) else {
        panic!("expected hardlinked journal refusal");
    };
    drop(error);
    assert_eq!(snapshot(root), before);
}
