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
fn source_writers_that_commit_or_checkpoint_between_capture_passes_are_rejected() {
    for checkpoint_after_write in [false, true] {
        let temp = tempdir().unwrap();
        let (source, writer) = create_source(temp.path(), false);
        #[expect(
            clippy::disallowed_methods,
            reason = "test fixture opens a tempdir source root"
        )]
        let root = fs::File::open(&source).unwrap();
        let result = OmpSnapshot::capture_from_root_inner(&root, deadline(), || {
            writer
                .execute(
                    "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                    [r#"{"key":"changed-during-capture"}"#],
                )
                .unwrap();
            if checkpoint_after_write {
                checkpoint(&writer);
            }
        });
        assert!(
            matches!(result, Err(OmpError::Unavailable)),
            "source mutation was accepted with checkpoint_after_write={checkpoint_after_write}"
        );
    }
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
