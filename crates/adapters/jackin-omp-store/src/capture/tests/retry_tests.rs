// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::retry::{AgentDirectorySecurityIdentity, CaptureFence, FileSecurityIdentity};
use super::super::test_hooks::{CapturePhase, set_capture_hook};
use super::*;
use std::cell::{Cell, RefCell};
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn pair_positions(phases: &[CapturePhase]) -> Vec<(usize, usize)> {
    phases
        .iter()
        .filter_map(|phase| match phase {
            CapturePhase::AfterPairRead { attempt, pass, .. } => Some((*attempt, *pass)),
            _ => None,
        })
        .collect()
}

fn assert_one_deadline(phases: &[CapturePhase]) {
    let mut deadlines = phases.iter().map(|phase| match phase {
        CapturePhase::BeforeDatabaseOpen { deadline, .. }
        | CapturePhase::AfterDatabaseRead { deadline, .. }
        | CapturePhase::AfterPairRead { deadline, .. } => *deadline,
    });
    let first = deadlines.next().expect("capture phases were observed");
    assert!(deadlines.all(|deadline| deadline == first));
}

#[test]
fn stable_source_succeeds_after_exactly_two_complete_pair_reads() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| phases_hook.borrow_mut().push(phase));
    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("source exists");
    drop(hook);

    assert_eq!(pair_positions(&phases.borrow()), vec![(0, 0), (0, 1)]);
    assert_one_deadline(&phases.borrow());
    let output = export_selected(&mut snapshot, 41);
    assert_exported_selected_row(
        &output,
        &temp.path().join("stable-export.db"),
        r#"{"key":"selected-old-canary"}"#,
    );
}

#[test]
fn real_full_checkpoint_between_database_and_wal_reads_retries_a_stable_pair() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), true);
    writer
        .execute(
            "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
            [r#"{"key":"selected-full-checkpoint-current-canary"}"#],
        )
        .unwrap();
    writer
        .execute_batch(
            "CREATE TABLE deleted_data(value TEXT);\
             INSERT INTO deleted_data(value) VALUES ('deleted-freelist-secret-canary');\
             DELETE FROM deleted_data;",
        )
        .unwrap();

    let database_path = source.join("agent/agent.db");
    let wal_path = source.join("agent/agent.db-wal");
    let database_before = fs::read(&database_path).unwrap();
    let database_identity = fs::metadata(&database_path).unwrap();
    assert!(fs::metadata(&wal_path).unwrap().len() > 0);

    let writer = Rc::new(writer);
    let writer_hook = Rc::clone(&writer);
    let checkpointed = Rc::new(Cell::new(false));
    let checkpointed_hook = Rc::clone(&checkpointed);
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        if matches!(
            phase,
            CapturePhase::AfterDatabaseRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            let (busy, log, copied) = full_checkpoint(&writer_hook);
            assert_eq!(busy, 0);
            assert!(log > 0);
            assert_eq!(copied, log);
            assert!(wal_path.exists());
            assert!(fs::metadata(&wal_path).unwrap().len() > 0);
            let database_after = fs::read(&database_path).unwrap();
            let identity_after = fs::metadata(&database_path).unwrap();
            assert_ne!(database_before, database_after);
            assert_eq!(database_identity.dev(), identity_after.dev());
            assert_eq!(database_identity.ino(), identity_after.ino());
            checkpointed_hook.set(true);
        }
    });
    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("the post-checkpoint pair is stable");
    drop(hook);

    assert!(checkpointed.get());
    assert_eq!(
        pair_positions(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)]
    );
    assert_one_deadline(&phases.borrow());
    let output = export_selected(&mut snapshot, 41);
    assert_exported_selected_row(
        &output,
        &temp.path().join("full-checkpoint-export.db"),
        r#"{"key":"selected-full-checkpoint-current-canary"}"#,
    );
    for forbidden in [
        b"selected-old-canary".as_slice(),
        b"same-provider-sibling-canary".as_slice(),
        b"other-access-canary".as_slice(),
        b"other-refresh-canary".as_slice(),
        b"unrelated-table-canary".as_slice(),
        b"deleted-freelist-secret-canary".as_slice(),
    ] {
        assert!(
            !output
                .windows(forbidden.len())
                .any(|part| part == forbidden)
        );
    }
    assert!(writer.is_autocommit());
}

#[test]
fn a_final_writer_close_between_complete_reads_allows_wal_removal() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    let agent_path = source.join("agent");
    let database_path = agent_path.join("agent.db");
    let wal_path = agent_path.join("agent.db-wal");
    let agent_before = fs::metadata(&agent_path).unwrap();
    let database_before = fs::metadata(&database_path).unwrap();
    let wal_before = fs::metadata(&wal_path).unwrap();
    assert!(wal_before.len() > 0);

    let mut writer = Some(writer);
    let closed = Rc::new(Cell::new(false));
    let closed_hook = Rc::clone(&closed);
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
            let connection = writer.take().expect("final fixture writer is present");
            connection
                .close()
                .map_err(|(_connection, error)| error)
                .expect("final fixture writer closes cleanly");
            assert!(!wal_path.exists());
            let agent_after = fs::metadata(&agent_path).unwrap();
            let database_after = fs::metadata(&database_path).unwrap();
            assert_eq!(agent_before.dev(), agent_after.dev());
            assert_eq!(agent_before.ino(), agent_after.ino());
            assert_eq!(agent_before.uid(), agent_after.uid());
            assert_eq!(agent_before.gid(), agent_after.gid());
            assert_eq!(agent_before.mode(), agent_after.mode());
            // Removing SQLite's WAL sidecar may change directory nlink across complete reads.
            assert_eq!(database_before.dev(), database_after.dev());
            assert_eq!(database_before.ino(), database_after.ino());
            assert_eq!(database_before.uid(), database_after.uid());
            assert_eq!(database_before.gid(), database_after.gid());
            assert_eq!(database_before.mode(), database_after.mode());
            assert_eq!(database_before.nlink(), database_after.nlink());
            closed_hook.set(true);
        }
    });
    let mut snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("stable database without a WAL remains capturable");
    drop(hook);

    assert!(closed.get());
    assert_eq!(
        pair_positions(&phases.borrow()),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)]
    );
    let output = export_selected(&mut snapshot, 41);
    assert_exported_selected_row(
        &output,
        &temp.path().join("writer-close-export.db"),
        r#"{"key":"selected-old-canary"}"#,
    );
}

#[test]
fn removing_wal_inside_a_pair_is_fatal_without_another_attempt() {
    let temp = tempdir().unwrap();
    let (source, _writer) = create_source(temp.path(), false);
    let wal_path = source.join("agent/agent.db-wal");
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| {
        phases_hook.borrow_mut().push(phase);
        if matches!(
            phase,
            CapturePhase::AfterDatabaseRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            fs::remove_file(&wal_path).unwrap();
        }
    });
    let result = OmpSnapshot::capture_from_directory(&source);
    drop(hook);

    assert!(matches!(result, Err(OmpError::Unavailable)));
    assert_eq!(
        phases
            .borrow()
            .iter()
            .filter(|phase| matches!(phase, CapturePhase::AfterDatabaseRead { .. }))
            .count(),
        1
    );
    assert!(pair_positions(&phases.borrow()).is_empty());
}

#[test]
fn one_deadline_is_reused_across_retry_and_an_expired_deadline_reads_nothing() {
    let temp = tempdir().unwrap();
    let (source, writer) = create_source(temp.path(), false);
    let writer = Rc::new(writer);
    let writer_hook = Rc::clone(&writer);
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
            writer_hook
                .execute(
                    "UPDATE auth_credentials SET data = ?1 WHERE id = 41",
                    [r#"{"key":"deadline-retry-canary"}"#],
                )
                .unwrap();
        }
    });
    let snapshot = OmpSnapshot::capture_from_directory(&source)
        .unwrap()
        .expect("one stable retry succeeds");
    drop(hook);
    assert_one_deadline(&phases.borrow());
    assert_eq!(pair_positions(&phases.borrow()).len(), 4);
    drop(snapshot);

    #[expect(
        clippy::disallowed_methods,
        reason = "test fixture opens a temporary source root"
    )]
    let root = fs::File::open(&source).unwrap();
    let expired_phases = Rc::new(RefCell::new(Vec::new()));
    let expired_hook_phases = Rc::clone(&expired_phases);
    let hook = set_capture_hook(move |phase| expired_hook_phases.borrow_mut().push(phase));
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        OmpSnapshot::capture_from_root_inner(&root, expired),
        Err(OmpError::Deadline)
    ));
    drop(hook);
    assert!(expired_phases.borrow().is_empty());
}

#[test]
fn stable_absence_is_none_and_source_transitions_fail_closed() {
    let temp = tempdir().unwrap();
    let source = temp.path().join(".omp");
    fs::create_dir(&source).unwrap();
    let phases = Rc::new(RefCell::new(Vec::new()));
    let phases_hook = Rc::clone(&phases);
    let hook = set_capture_hook(move |phase| phases_hook.borrow_mut().push(phase));
    assert!(
        OmpSnapshot::capture_from_directory(&source)
            .unwrap()
            .is_none()
    );
    drop(hook);
    assert_eq!(pair_positions(&phases.borrow()), vec![(0, 0), (0, 1)]);

    let temp_appearing = tempdir().unwrap();
    let appearing_source = temp_appearing.path().join(".omp");
    let appearing_parent = temp_appearing.path().to_path_buf();
    let appearing_source_hook = appearing_source.clone();
    fs::create_dir(&appearing_source).unwrap();
    let created_writer = Rc::new(RefCell::new(None));
    let created_writer_hook = Rc::clone(&created_writer);
    let created = Rc::new(Cell::new(false));
    let created_hook = Rc::clone(&created);
    let hook = set_capture_hook(move |phase| {
        if matches!(
            phase,
            CapturePhase::AfterPairRead {
                attempt: 0,
                pass: 0,
                ..
            }
        ) {
            let (created_source, writer) = create_source(&appearing_parent, false);
            assert_eq!(created_source, appearing_source_hook);
            *created_writer_hook.borrow_mut() = Some(writer);
            created_hook.set(true);
        }
    });
    let result = OmpSnapshot::capture_from_directory(&appearing_source);
    drop(hook);
    assert!(created.get());
    assert!(matches!(result, Err(OmpError::Unavailable)));
    drop(created_writer);

    let temp_deleted = tempdir().unwrap();
    let (deleted_source, _writer) = create_source(temp_deleted.path(), false);
    let agent = deleted_source.join("agent");
    let detached = deleted_source.join("agent.detached");
    let deleted = Rc::new(Cell::new(false));
    let deleted_hook = Rc::clone(&deleted);
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
            fs::rename(&agent, &detached).unwrap();
            deleted_hook.set(true);
        }
    });
    let result = OmpSnapshot::capture_from_directory(&deleted_source);
    drop(hook);
    assert!(deleted.get());
    assert!(matches!(result, Err(OmpError::Unavailable)));
    let positions = pair_positions(&phases.borrow());
    assert!(
        matches!(positions.as_slice(), [(0, 0)] | [(0, 0), (0, 1)]),
        "unexpected completed read positions: {positions:?}"
    );
}

#[test]
fn security_fence_keys_every_file_authority_field_but_not_mutable_metadata() {
    let identity = super::super::FileIdentity {
        device: 10,
        inode: 20,
        uid: 30,
        gid: 40,
        mode: 0o100600,
        links: 1,
        size: 512,
        modified: 60,
        changed: 70,
    };
    let expected = FileSecurityIdentity::from(identity);
    let mut mutations = Vec::new();
    let mut changed = identity;
    changed.device += 1;
    mutations.push(changed);
    changed = identity;
    changed.inode += 1;
    mutations.push(changed);
    changed = identity;
    changed.uid += 1;
    mutations.push(changed);
    changed = identity;
    changed.gid += 1;
    mutations.push(changed);
    changed = identity;
    changed.mode ^= 0o010;
    mutations.push(changed);
    changed = identity;
    changed.links += 1;
    mutations.push(changed);
    assert!(
        mutations
            .into_iter()
            .all(|mutation| FileSecurityIdentity::from(mutation) != expected)
    );

    let mut mutable = identity;
    mutable.size += 1;
    mutable.modified += 1;
    mutable.changed += 1;
    assert_eq!(FileSecurityIdentity::from(mutable), expected);
}

#[test]
fn agent_directory_fence_ignores_mutable_metadata_and_rejects_security_changes() {
    let agent_identity = super::super::FileIdentity {
        device: 10,
        inode: 20,
        uid: 30,
        gid: 40,
        mode: 0o040700,
        links: 2,
        size: 512,
        modified: 60,
        changed: 70,
    };
    let database_identity = super::super::FileIdentity {
        device: 11,
        inode: 21,
        uid: 31,
        gid: 41,
        mode: 0o100600,
        links: 1,
        size: 1024,
        modified: 61,
        changed: 71,
    };

    let mut mutable_pair = super::super::SourcePair {
        agent_identity,
        database: zeroize::Zeroizing::new(Vec::new()),
        database_identity,
        wal: None,
        wal_identity: None,
    };
    let mut fence = CaptureFence::default();
    fence
        .observe(Some(&mutable_pair))
        .expect("the first complete pair establishes the fence");
    mutable_pair.agent_identity.links += 1;
    mutable_pair.agent_identity.size += 1;
    mutable_pair.agent_identity.modified += 1;
    mutable_pair.agent_identity.changed += 1;
    fence
        .observe(Some(&mutable_pair))
        .expect("directory link and mutable metadata may change across complete reads");

    let mut agent_mutations = Vec::new();
    let mut changed = agent_identity;
    changed.device += 1;
    agent_mutations.push(changed);
    changed = agent_identity;
    changed.inode += 1;
    agent_mutations.push(changed);
    changed = agent_identity;
    changed.uid += 1;
    agent_mutations.push(changed);
    changed = agent_identity;
    changed.gid += 1;
    agent_mutations.push(changed);
    changed = agent_identity;
    changed.mode ^= 0o010;
    agent_mutations.push(changed);
    changed = agent_identity;
    changed.mode = 0o100700;
    agent_mutations.push(changed);

    for changed_agent in agent_mutations {
        let mut pair = super::super::SourcePair {
            agent_identity,
            database: zeroize::Zeroizing::new(Vec::new()),
            database_identity,
            wal: None,
            wal_identity: None,
        };
        let mut fence = CaptureFence::default();
        fence
            .observe(Some(&pair))
            .expect("the first complete pair establishes the fence");
        pair.agent_identity = changed_agent;
        let error = fence
            .observe(Some(&pair))
            .expect_err("agent directory security identity changes fail closed");
        assert!(matches!(error, OmpError::Unavailable));
    }

    let mut database_mutations = Vec::new();
    let mut changed_database = database_identity;
    changed_database.device += 1;
    database_mutations.push(changed_database);
    changed_database = database_identity;
    changed_database.inode += 1;
    database_mutations.push(changed_database);
    changed_database = database_identity;
    changed_database.uid += 1;
    database_mutations.push(changed_database);
    changed_database = database_identity;
    changed_database.gid += 1;
    database_mutations.push(changed_database);
    changed_database = database_identity;
    changed_database.mode ^= 0o010;
    database_mutations.push(changed_database);
    changed_database = database_identity;
    changed_database.links += 1;
    database_mutations.push(changed_database);

    for changed_database in database_mutations {
        let mut pair = super::super::SourcePair {
            agent_identity,
            database: zeroize::Zeroizing::new(Vec::new()),
            database_identity,
            wal: None,
            wal_identity: None,
        };
        let mut fence = CaptureFence::default();
        fence
            .observe(Some(&pair))
            .expect("the first complete pair establishes the fence");
        pair.database_identity = changed_database;
        let error = fence
            .observe(Some(&pair))
            .expect_err("regular database security identity changes fail closed");
        assert!(matches!(error, OmpError::Unavailable));
    }

    assert_eq!(
        AgentDirectorySecurityIdentity::from(agent_identity),
        AgentDirectorySecurityIdentity::from(mutable_pair.agent_identity)
    );
}
