// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[cfg(unix)]
fn write_omp_fixture(source: &Path, selected: &str) -> rusqlite::Connection {
    let writer = write_v7_omp_source(source, OMP_V7_SCHEMA);
    writer
        .execute(
            "INSERT INTO auth_credentials (id, provider, credential_type, data) \
             VALUES (41, 'openai', 'api_key', ?1)",
            [selected],
        )
        .unwrap();
    writer
}

#[cfg(unix)]
fn close_omp_fixture(writer: rusqlite::Connection) {
    let _: (i64, i64, i64) = writer
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    drop(writer);
}

#[cfg(unix)]
fn write_private_target(target: &Path, contents: &[u8]) {
    std::fs::create_dir_all(target.parent().expect("target has a parent"))
        .expect("target parent is created");
    std::fs::write(target, contents).expect("existing target is written");
    std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o600))
        .expect("existing target is private");
    assert_eq!(
        std::fs::metadata(target)
            .expect("target metadata is readable")
            .mode()
            & 0o777,
        0o600
    );
}

#[cfg(unix)]
fn provision_omp(
    source: &Path,
    target: &Path,
) -> anyhow::Result<(AuthProvisionOutcome, Option<PathBuf>)> {
    provision_omp_auth_from_source_dir(
        target,
        AuthForwardMode::Sync,
        source,
        Some(AiProvider::OpenAi),
        Some(&omp_row_selector("openai", 41)),
    )
}

#[cfg(unix)]
fn captured_pairs(phases: &[jackin_omp_store::CapturePhase]) -> Vec<(usize, usize)> {
    phases
        .iter()
        .filter_map(|phase| match phase {
            jackin_omp_store::CapturePhase::AfterPairRead { attempt, pass, .. } => {
                Some((*attempt, *pass))
            }
            _ => None,
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn omp_agent_db_replacement_between_validation_and_open_is_rejected() {
    use jackin_omp_store::{CapturePhase, set_capture_hook};
    use std::cell::Cell;
    use std::rc::Rc;

    let temp = tempdir().unwrap();
    let baseline_source = temp.path().join("baseline/.omp");
    close_omp_fixture(write_omp_fixture(
        &baseline_source,
        r#"{"key":"baseline-selected"}"#,
    ));
    let baseline_target = temp.path().join("baseline-role/omp/agent.db");
    std::fs::create_dir_all(baseline_target.parent().unwrap()).unwrap();
    assert_eq!(
        provision_omp(&baseline_source, &baseline_target).unwrap().0,
        AuthProvisionOutcome::Synced
    );

    let source = temp.path().join("race/.omp");
    close_omp_fixture(write_omp_fixture(
        &source,
        r#"{"key":"race-original-selected"}"#,
    ));
    let replacement_source = temp.path().join("replacement/.omp");
    close_omp_fixture(write_omp_fixture(
        &replacement_source,
        r#"{"key":"race-replacement-selected"}"#,
    ));
    let database = source.join("agent/agent.db");
    let replacement = replacement_source.join("agent/agent.db");
    assert!(std::fs::metadata(&replacement).unwrap().is_file());
    let replacement_connection = rusqlite::Connection::open(&replacement).unwrap();
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
    let original_inode = std::fs::metadata(&database).unwrap().ino();
    let replacement_inode = std::fs::metadata(&replacement).unwrap().ino();
    assert_ne!(original_inode, replacement_inode);
    let displaced = source.join("agent/agent.db.displaced");
    let database_hook = database.clone();
    let replacement_hook = replacement.clone();
    let displaced_hook = displaced.clone();
    let hook_calls = Rc::new(Cell::new(0));
    let hook_count = Rc::clone(&hook_calls);
    let hook = set_capture_hook(move |phase| {
        if matches!(
            phase,
            CapturePhase::BeforeDatabaseOpen {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            hook_count.set(hook_count.get() + 1);
            std::fs::rename(&database_hook, &displaced_hook).unwrap();
            std::fs::rename(&replacement_hook, &database_hook).unwrap();
        }
    });
    let target = temp.path().join("race-role/omp/agent.db");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let result = provision_omp(&source, &target);
    drop(hook);

    assert_eq!(hook_calls.get(), 1);
    let error = result.expect_err("the nofollow stat/open identity race must fail closed");
    assert!(format!("{error:#}").contains("OMP credential source is unavailable"));
    assert!(!target.exists(), "a leaf race must not publish a target");
    assert_eq!(
        std::fs::metadata(database).unwrap().ino(),
        replacement_inode
    );
}

#[cfg(unix)]
#[test]
fn omp_retry_publishes_only_the_stable_selected_row_after_a_committed_change() {
    use jackin_omp_store::{CapturePhase, set_capture_hook};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let temp = tempdir().unwrap();
    let source = temp.path().join("source/.omp");
    let writer = Rc::new(write_omp_fixture(
        &source,
        r#"{"key":"selected-before-retry"}"#,
    ));
    let target = temp.path().join("role/omp/agent.db");
    let previous = b"preexisting-target-must-stay-until-capture-stabilizes".to_vec();
    write_private_target(&target, &previous);
    let writer_hook = Rc::clone(&writer);
    let target_hook = target.clone();
    let previous_hook = previous.clone();
    let changed = Rc::new(Cell::new(false));
    let changed_hook = Rc::clone(&changed);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        assert_eq!(std::fs::read(&target_hook).unwrap(), previous_hook);
        phases_hook.borrow_mut().push(phase);
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
                    [r#"{"key":"selected-after-retry"}"#],
                )
                .unwrap();
            changed_hook.set(true);
        }
    });
    let result = provision_omp(&source, &target);
    drop(hook);

    let (outcome, mounted) = result.unwrap();
    assert!(changed.get());
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(target.as_path()));
    assert_eq!(
        captured_pairs(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)]
    );
    let published = rusqlite::Connection::open(&target).unwrap();
    let mut statement = published
        .prepare("SELECT id, data FROM auth_credentials ORDER BY id")
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![(41, r#"{"key":"selected-after-retry"}"#.to_owned())]
    );
}

#[cfg(unix)]
#[test]
fn omp_later_inode_change_preserves_an_existing_target_byte_for_byte() {
    use jackin_omp_store::{CapturePhase, set_capture_hook};
    use std::cell::RefCell;
    use std::rc::Rc;

    let temp = tempdir().unwrap();
    let source = temp.path().join("source/.omp");
    let writer = write_omp_fixture(&source, r#"{"key":"selected-before-security-failure"}"#);
    let replacement_source = temp.path().join("replacement/.omp");
    close_omp_fixture(write_omp_fixture(
        &replacement_source,
        r#"{"key":"replacement-is-valid-but-unbound"}"#,
    ));
    let database = source.join("agent/agent.db");
    let wal = source.join("agent/agent.db-wal");
    let replacement = replacement_source.join("agent/agent.db");
    let replacement_inode = std::fs::metadata(&replacement).unwrap().ino();
    let displaced = source.join("agent/agent.db.displaced");
    let mut writer = Some(writer);
    let database_hook = database.clone();
    let wal_hook = wal.clone();
    let replacement_hook = replacement.clone();
    let displaced_hook = displaced.clone();
    let target = temp.path().join("role/omp/agent.db");
    let previous = b"exact-existing-target-bytes".to_vec();
    write_private_target(&target, &previous);
    let target_hook = target.clone();
    let previous_hook = previous.clone();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        assert_eq!(std::fs::read(&target_hook).unwrap(), previous_hook);
        phases_hook.borrow_mut().push(phase);
        match phase {
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            } => {
                writer
                    .as_ref()
                    .expect("fixture writer remains open")
                    .execute(
                        "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                        [r#"{"key":"eligible-change-before-security-failure"}"#],
                    )
                    .unwrap();
            }
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 1,
                ..
            } => {
                let writer = writer.take().expect("fixture writer remains open");
                writer
                    .close()
                    .map_err(|(_connection, error)| error)
                    .expect("fixture writer closes cleanly");
                if wal_hook.exists() {
                    std::fs::remove_file(&wal_hook).unwrap();
                }
                std::fs::rename(&database_hook, &displaced_hook).unwrap();
                std::fs::rename(&replacement_hook, &database_hook).unwrap();
            }
            _ => {}
        }
    });
    let result = provision_omp(&source, &target);
    drop(hook);

    let error = result.expect_err("the later unbound database inode must fail closed");
    assert!(format!("{error:#}").contains("OMP credential source is unavailable"));
    assert_eq!(std::fs::read(&target).unwrap(), previous);
    assert_eq!(
        captured_pairs(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0)]
    );
    assert_eq!(
        std::fs::metadata(database).unwrap().ino(),
        replacement_inode
    );
}

#[cfg(unix)]
#[test]
fn omp_database_disappearance_after_eligible_change_preserves_existing_target() {
    use jackin_omp_store::{CapturePhase, set_capture_hook};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let temp = tempdir().unwrap();
    let source = temp.path().join("source/.omp");
    let writer = Rc::new(RefCell::new(Some(write_omp_fixture(
        &source,
        r#"{"key":"selected-before-database-removal"}"#,
    ))));
    let database = source.join("agent/agent.db");
    let wal = source.join("agent/agent.db-wal");
    let target = temp.path().join("role/omp/agent.db");
    let previous = b"existing-private-target-survives-database-removal".to_vec();
    write_private_target(&target, &previous);
    let writer_hook = Rc::clone(&writer);
    let database_hook = database.clone();
    let wal_hook = wal.clone();
    let target_hook = target.clone();
    let previous_hook = previous.clone();
    let changed = Rc::new(Cell::new(false));
    let changed_hook = Rc::clone(&changed);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        assert_eq!(std::fs::read(&target_hook).unwrap(), previous_hook);
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
                std::fs::remove_file(&database_hook).unwrap();
            }
            _ => {}
        }
    });
    let result = provision_omp(&source, &target);
    drop(hook);

    let error = result.expect_err("database disappearance must fail closed");
    assert!(format!("{error:#}").contains("OMP credential source is unavailable"));
    assert!(changed.get());
    assert!(writer.borrow().is_none());
    assert!(!database.exists());
    assert_eq!(std::fs::read(&target).unwrap(), previous);
    assert_eq!(std::fs::metadata(&target).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        captured_pairs(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0)]
    );
    let entries = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(entries, vec!["agent.db".to_owned()]);
}

#[cfg(unix)]
#[test]
fn omp_continuous_committed_churn_stops_at_three_attempts_without_publication() {
    use jackin_omp_store::{CapturePhase, set_capture_hook};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let temp = tempdir().unwrap();
    let source = temp.path().join("source/.omp");
    let writer = Rc::new(write_omp_fixture(
        &source,
        r#"{"key":"selected-before-churn"}"#,
    ));
    let target = temp.path().join("role/omp/agent.db");
    let previous = b"existing-target-survives-exhausted-capture".to_vec();
    write_private_target(&target, &previous);
    let writer_hook = Rc::clone(&writer);
    let target_hook = target.clone();
    let previous_hook = previous.clone();
    let churn_count = Rc::new(Cell::new(0));
    let churn_count_hook = Rc::clone(&churn_count);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        assert_eq!(std::fs::read(&target_hook).unwrap(), previous_hook);
        phases_hook.borrow_mut().push(phase);
        if let CapturePhase::AfterPairRead {
            attempt, pass: 0, ..
        } = phase
        {
            let next = churn_count_hook.get() + 1;
            writer_hook
                .execute(
                    "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                    [format!(r#"{{"key":"committed-churn-{attempt}-{next}"}}"#)],
                )
                .unwrap();
            churn_count_hook.set(next);
        }
    });
    let result = provision_omp(&source, &target);
    drop(hook);

    let error = result.expect_err("three unstable captures must exhaust the retry cap");
    assert!(format!("{error:#}").contains("OMP credential source is unavailable"));
    assert_eq!(churn_count.get(), 3);
    assert_eq!(
        captured_pairs(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
    );
    assert_eq!(std::fs::read(&target).unwrap(), previous);
    let entries = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(entries, vec!["agent.db".to_owned()]);
}
