// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::test_hooks::{CapturePhase, set_capture_hook};
use super::*;
use std::cell::{Cell, RefCell};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::rc::Rc;

fn replacement_database(parent: &Path) -> std::path::PathBuf {
    let (replacement_source, writer) = create_source(parent, false);
    checkpoint(&writer);
    drop(writer);
    replacement_source.join("agent/agent.db")
}

fn pair_positions(phases: &[CapturePhase]) -> Vec<(usize, usize)> {
    phases
        .iter()
        .filter_map(|phase| match phase {
            CapturePhase::AfterPairRead { attempt, pass, .. } => Some((*attempt, *pass)),
            _ => None,
        })
        .collect()
}

#[test]
fn database_leaf_replacement_after_nofollow_stat_is_fatal_without_retry() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    let database = source.join("agent/agent.db");
    let replacement = replacement_database(&temp.path().join("replacement"));
    let replacement_connection = Connection::open(&replacement).unwrap();
    assert_eq!(
        replacement_connection
            .query_row(
                "SELECT version FROM auth_schema_version WHERE id = 1",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
        7
    );
    drop(replacement_connection);

    let original_inode = fs::metadata(&database).unwrap().ino();
    let before_open = Rc::new(Cell::new(0));
    let before_open_hook = Rc::clone(&before_open);
    let database_hook = database.clone();
    let replacement_hook = replacement.clone();
    let displaced = source.join("agent/agent.db.displaced");
    let displaced_hook = displaced.clone();
    let hook = set_capture_hook(move |phase| {
        if matches!(
            phase,
            CapturePhase::BeforeDatabaseOpen {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            before_open_hook.set(before_open_hook.get() + 1);
            fs::rename(&database_hook, &displaced_hook).unwrap();
            fs::rename(&replacement_hook, &database_hook).unwrap();
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert_eq!(before_open.get(), 1);
    assert_ne!(original_inode, fs::metadata(&database).unwrap().ino());
}

#[test]
fn database_replacement_on_a_later_attempt_is_rejected_by_the_persistent_fence() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    let agent = source.join("agent");
    let database = agent.join("agent.db");
    let wal = agent.join("agent.db-wal");
    let replacement = replacement_database(&temp.path().join("replacement"));
    let mut writer = Some(writer);
    let original_inode = fs::metadata(&database).unwrap().ino();
    let replacement_inode = fs::metadata(&replacement).unwrap().ino();
    assert_ne!(original_inode, replacement_inode);
    let changed = Rc::new(Cell::new(false));
    let changed_hook = Rc::clone(&changed);
    let replaced = Rc::new(Cell::new(false));
    let replaced_hook = Rc::clone(&replaced);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let database_hook = database.clone();
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        match phase {
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            } => {
                writer
                    .as_ref()
                    .expect("source writer is present")
                    .execute(
                        "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                        [r#"{"key":"eligible-first-attempt-change"}"#],
                    )
                    .unwrap();
                changed_hook.set(true);
            }
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 1,
                ..
            } => {
                let connection = writer.take().expect("source writer is present");
                connection
                    .close()
                    .map_err(|(_connection, error)| error)
                    .expect("source writer closes cleanly");
                if wal.exists() {
                    fs::remove_file(&wal).unwrap();
                }
                fs::rename(&database_hook, agent.join("agent.db.displaced")).unwrap();
                fs::rename(&replacement, &database_hook).unwrap();
                replaced_hook.set(true);
            }
            _ => {}
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert!(changed.get());
    assert!(replaced.get());
    assert_eq!(
        pair_positions(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0)]
    );
    assert_eq!(fs::metadata(database).unwrap().ino(), replacement_inode);
}

#[test]
fn database_disappearance_after_eligible_pair_stops_before_another_read() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    let agent = source.join("agent");
    let database = agent.join("agent.db");
    let wal = agent.join("agent.db-wal");
    let writer = Rc::new(RefCell::new(Some(writer)));
    let writer_hook = Rc::clone(&writer);
    let changed = Rc::new(Cell::new(false));
    let changed_hook = Rc::clone(&changed);
    let removed = Rc::new(Cell::new(false));
    let removed_hook = Rc::clone(&removed);
    let database_hook = database.clone();
    let wal_hook = wal.clone();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        match phase {
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            } => {
                assert_eq!(
                    writer_hook
                        .borrow()
                        .as_ref()
                        .expect("source writer is present")
                        .execute(
                            "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                            [r#"{"key":"eligible-change-before-database-removal"}"#],
                        )
                        .unwrap(),
                    1
                );
                changed_hook.set(true);
            }
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 1,
                ..
            } => {
                let connection = writer_hook
                    .borrow_mut()
                    .take()
                    .expect("final source writer is present");
                connection
                    .close()
                    .map_err(|(_connection, error)| error)
                    .expect("final source writer closes cleanly");
                assert!(!wal_hook.exists());
                fs::remove_file(&database_hook).unwrap();
                assert!(!database_hook.exists());
                removed_hook.set(true);
            }
            _ => {}
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(changed.get());
    assert!(removed.get());
    assert!(writer.borrow().is_none());
    assert!(agent.is_dir());
    assert!(!database.exists());
    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert_eq!(
        pair_positions(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0)]
    );
}

#[test]
fn changed_file_mode_is_fatal_even_when_secure_reads_complete() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    let database = source.join("agent/agent.db");
    let database_hook = database.clone();
    let initial_mode = 0o600;
    let changed_mode = 0o640;
    fs::set_permissions(&database, fs::Permissions::from_mode(initial_mode)).unwrap();
    assert_eq!(
        fs::metadata(&database).unwrap().mode() & 0o777,
        initial_mode
    );
    let changed = Rc::new(Cell::new(false));
    let changed_hook = Rc::clone(&changed);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        if matches!(
            phase,
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            fs::set_permissions(&database_hook, fs::Permissions::from_mode(changed_mode)).unwrap();
            assert_eq!(
                fs::metadata(&database_hook).unwrap().mode() & 0o777,
                changed_mode
            );
            changed_hook.set(true);
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert!(changed.get());
    assert_ne!(initial_mode, changed_mode);
    assert_eq!(pair_positions(&phases.borrow()), vec![(0, 0), (0, 1)]);
}

#[test]
fn replacing_a_wal_inode_with_the_same_bytes_is_fatal() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    let wal = source.join("agent/agent.db-wal");
    let original_bytes = fs::read(&wal).unwrap();
    let original_inode = fs::metadata(&wal).unwrap().ino();
    let replacement = source.join("agent/agent.db-wal.replacement");
    let wal_hook = wal.clone();
    let replacement_hook = replacement.clone();
    let bytes_hook = original_bytes.clone();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        if matches!(
            phase,
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            fs::write(&replacement_hook, &bytes_hook).unwrap();
            fs::rename(&replacement_hook, &wal_hook).unwrap();
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert_eq!(pair_positions(&phases.borrow()), vec![(0, 0), (0, 1)]);
    assert_ne!(original_inode, fs::metadata(&wal).unwrap().ino());
}

#[test]
fn wal_disappearance_then_reappearance_does_not_bind_a_new_inode() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    let wal = source.join("agent/agent.db-wal");
    let original_bytes = fs::read(&wal).unwrap();
    let original_inode = fs::metadata(&wal).unwrap().ino();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let wal_hook = wal.clone();
    let bytes_hook = original_bytes.clone();
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        match phase {
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            } => {
                fs::remove_file(&wal_hook).unwrap();
            }
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 1,
                ..
            } => {
                fs::write(&wal_hook, &bytes_hook).unwrap();
            }
            _ => {}
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert_eq!(
        pair_positions(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0)]
    );
    assert_ne!(original_inode, fs::metadata(&wal).unwrap().ino());
}

#[test]
fn wal_appearing_after_an_absent_first_pair_is_rejected() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    let wal = source.join("agent/agent.db-wal");
    let saved_wal = fs::read(&wal).unwrap();
    checkpoint(&writer);
    drop(writer);
    if wal.exists() {
        fs::remove_file(&wal).unwrap();
    }
    let wal_hook = wal.clone();
    let saved_wal_hook = saved_wal.clone();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        if matches!(
            phase,
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            fs::write(&wal_hook, &saved_wal_hook).unwrap();
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert_eq!(pair_positions(&phases.borrow()), vec![(0, 0), (0, 1)]);
}
