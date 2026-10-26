// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn selected_role_database_contains_only_the_bound_omp_credential_row() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), true);
    writer
        .execute_batch(
            "CREATE TABLE deleted_data(value TEXT);
                 INSERT INTO deleted_data(value)
                 VALUES ('deleted-freelist-secret-canary');
                 DELETE FROM deleted_data;",
        )
        .unwrap();
    drop(writer);
    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("source exists");
    let selected = snapshot
        .select(Some("openai"), Some(&selector(41)))
        .unwrap();
    let mut output = zeroize::Zeroizing::new(Vec::new());
    selected.write_standalone_database(&mut *output).unwrap();
    assert!(
        !output
            .windows(b"deleted-freelist-secret-canary".len())
            .any(|bytes| { bytes == b"deleted-freelist-secret-canary" })
    );
}

#[test]
fn source_commit_between_complete_pair_reads_retries_the_new_committed_row() {
    use super::super::test_hooks::{CapturePhase, set_capture_hook};
    use std::cell::Cell;
    use std::rc::Rc;

    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    let writer = Rc::new(writer);
    let writer_hook = Rc::clone(&writer);
    let changed = Rc::new(Cell::new(false));
    let changed_hook = Rc::clone(&changed);
    let hook = set_capture_hook(move |phase| {
        if matches!(
            phase,
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            writer_hook
                .execute(
                    "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                    [r#"{"key":"changed-during-capture"}"#],
                )
                .unwrap();
            changed_hook.set(true);
        }
    });
    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("source exists");
    drop(hook);

    assert!(changed.get());
    let output = export_selected(&mut snapshot, 41);
    assert_exported_selected_row(
        &output,
        &temp.path().join("selected-export.db"),
        r#"{"key":"changed-during-capture"}"#,
    );
}

#[test]
fn stable_present_empty_wal_after_truncate_remains_rejected() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    checkpoint(&writer);
    let wal_path = source.join("agent/agent.db-wal");
    assert!(wal_path.exists());
    assert_eq!(fs::metadata(wal_path).unwrap().len(), 0);

    let result = OmpSnapshot::capture_from_directory(&source);
    assert!(matches!(result, Err(OmpError::Unavailable)));
}

#[test]
fn a_stable_uncommitted_wal_spill_exports_the_last_committed_selected_row() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    write_uncommitted_spill(&writer);
    let database = fs::read(source.join("agent/agent.db")).unwrap();
    let wal = fs::read(source.join("agent/agent.db-wal")).unwrap();
    let header = validate_database(&database, deadline()).unwrap();
    let wal_summary = validate_wal(&wal, header, deadline()).unwrap();
    let last_commit = wal_summary
        .last_commit
        .expect("fixture has a committed row");
    assert!(last_commit.final_frame + 1 < wal_summary.frame_count);

    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("source exists");
    assert_eq!(
        snapshot.accounts(),
        &[OmpAccount {
            id: 41,
            entry: "openai".to_owned(),
            profile: "row:41".to_owned(),
        }]
    );
    let selected = snapshot
        .select(Some("openai"), Some(&selector(41)))
        .unwrap();
    let mut output = zeroize::Zeroizing::new(Vec::new());
    selected.write_standalone_database(&mut *output).unwrap();
    assert!(
        output
            .windows(b"selected-old-canary".len())
            .any(|bytes| { bytes == b"selected-old-canary" })
    );
    assert!(
        !output
            .windows(b"uncommitted-spill".len())
            .any(|bytes| { bytes == b"uncommitted-spill" })
    );
    writer.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn missing_or_stale_exact_selectors_never_fall_back_to_a_sibling() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), true);
    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("source exists");
    assert_eq!(
        snapshot.select(Some("openai"), None).err(),
        Some(OmpError::SelectionUnavailable)
    );
    assert_eq!(
        snapshot.select(Some("openai"), Some(&selector(99))).err(),
        Some(OmpError::SelectionUnavailable)
    );
    let selected = snapshot
        .select(Some("openai"), Some(&selector(42)))
        .unwrap();
    assert_eq!(selected.account().id, 42);
}

#[test]
fn private_database_cleanup_removes_all_secret_bearing_sidecars() {
    let temp = tempdir().unwrap();
    let database = temp.path().join("selected-role.db");
    fs::write(&database, b"synthetic-selected-secret").unwrap();
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = database.as_os_str().to_owned();
        sidecar.push(suffix);
        fs::write(sidecar, b"synthetic-private-sidecar").unwrap();
    }
    let mut cleanup = PrivateDatabaseCleanup::new(database.clone());
    cleanup.cleanup().unwrap();
    assert!(!database.exists());
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = database.as_os_str().to_owned();
        sidecar.push(suffix);
        assert!(!std::path::PathBuf::from(sidecar).exists());
    }
}

#[test]
fn successful_capture_cleanup_removes_the_private_role_database() {
    let temp = tempdir().unwrap();
    let database = temp.path().join("selected-role.db");
    fs::write(&database, b"synthetic-selected-secret").unwrap();
    let mut cleanup = PrivateDatabaseCleanup::new(database.clone());
    cleanup.cleanup().unwrap();
    drop(cleanup);
    assert!(!database.exists());
}

#[test]
fn close_errors_are_not_accepted() {
    assert!(close_succeeded(Ok(())));
    let connection = Connection::open_in_memory().unwrap();
    assert!(!close_succeeded(Err((
        connection,
        rusqlite::Error::InvalidQuery,
    ))));
}

#[test]
fn shared_file_descriptor_remains_open_after_a_read_only_capture() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    #[expect(
        clippy::disallowed_methods,
        reason = "test fixture opens a tempdir source root"
    )]
    let root = fs::File::open(source).unwrap();
    let snapshot = OmpSnapshot::capture_from_root(&root)
        .unwrap()
        .expect("source exists");
    root.as_fd().try_clone_to_owned().unwrap();
    assert_eq!(AUTH_SCHEMA_VERSION, 7);
    drop(snapshot);
}
