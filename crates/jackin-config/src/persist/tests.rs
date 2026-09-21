// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::sync::mpsc;

#[test]
fn config_lock_two_writers_serialize() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let first = acquire_lock(&config, LockMode::Exclusive, Duration::ZERO, Duration::ZERO).unwrap();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let config_for_thread = config.clone();
    let waiter = std::thread::spawn(move || {
        let second = acquire_lock(
            &config_for_thread,
            LockMode::Exclusive,
            Duration::from_secs(1),
            Duration::from_millis(1),
        )
        .unwrap();
        acquired_tx.send(()).unwrap();
        second
    });
    assert!(acquired_rx.recv_timeout(Duration::from_millis(20)).is_err());
    drop(first);
    acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(waiter.join().unwrap());
}

#[test]
fn config_lock_shared_reader_excludes_writer() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let reader = acquire_lock(&config, LockMode::Shared, Duration::ZERO, Duration::ZERO).unwrap();
    let err =
        acquire_lock(&config, LockMode::Exclusive, Duration::ZERO, Duration::ZERO).unwrap_err();
    assert!(matches!(err, crate::ConfigError::ConfigLockTimeout { .. }));
    drop(reader);
    drop(acquire_lock(&config, LockMode::Exclusive, Duration::ZERO, Duration::ZERO).unwrap());
}

#[test]
fn config_lock_timeout_is_typed_and_reports_recorded_pid() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let writer = acquire_config_write_lock(&config).unwrap();
    let err = acquire_lock(&config, LockMode::Shared, Duration::ZERO, Duration::ZERO).unwrap_err();
    assert!(matches!(err, crate::ConfigError::ConfigLockTimeout { .. }));
    assert!(err.to_string().contains(&std::process::id().to_string()));
    drop(writer);
}

#[test]
fn config_lock_timeout_uses_injected_clock_without_sleeping() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let writer = acquire_config_write_lock(&config).unwrap();
    let mut ticks = [Duration::ZERO, Duration::from_millis(2)].into_iter();
    let mut waits = Vec::new();
    let err = acquire_lock_with_timing(
        &config,
        LockMode::Shared,
        Duration::from_millis(1),
        Duration::from_millis(1),
        || ticks.next().unwrap_or(Duration::from_millis(2)),
        |duration| waits.push(duration),
    )
    .unwrap_err();
    assert!(matches!(err, crate::ConfigError::ConfigLockTimeout { .. }));
    assert_eq!(waits, [Duration::from_millis(1)]);
    drop(writer);
}

#[test]
#[cfg(unix)]
// Re-spawns the test binary as a lock-holding child that must die with a
// real OS signal; process spawning and kernel flock semantics are outside
// what Miri models (posix spawn attributes are unsupported), so skip under
// Miri.
#[cfg_attr(miri, ignore)]
fn config_lock_process_death_releases_ownership() {
    const CHILD_PATH: &str = "JACKIN_CONFIG_LOCK_TEST_CHILD";
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        let config = PathBuf::from(path);
        let _writer = acquire_config_write_lock(&config).unwrap();
        std::fs::write(config.with_extension("ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(Duration::from_mins(1));
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let ready = config.with_extension("ready");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("persist::tests::config_lock_process_death_releases_ownership")
        .arg("--nocapture")
        .env(CHILD_PATH, &config)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready.exists(), "child did not acquire config lock");
    child.kill().unwrap();
    child.wait().unwrap();

    let lock_path = config.with_file_name("config.lock");
    assert!(lock_path.exists(), "persistent lock file must remain");
    drop(
        acquire_lock(
            &config,
            LockMode::Exclusive,
            Duration::from_secs(1),
            Duration::from_millis(1),
        )
        .unwrap(),
    );
}

fn staged_config_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let workspace = temp.path().join("workspaces").join("ws.toml");
    std::fs::create_dir_all(workspace.parent().unwrap()).unwrap();
    std::fs::write(&config, "global-old").unwrap();
    std::fs::write(&workspace, "ws-old").unwrap();
    (temp, config, workspace)
}

fn staged_leftovers(dir: &Path) -> Vec<PathBuf> {
    let mut leftovers = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".tmp."))
        {
            leftovers.push(path);
        }
    }
    leftovers
}

#[test]
fn crash_between_renames_recovered_on_next_write_lock() {
    let (temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);

    // Simulate `kill -9` after the first of two renames: journal durable,
    // one rename applied, `Drop` cleanup skipped via `forget` (a dead
    // process runs no destructors).
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let deletes = Vec::new();
    write_publication_journal(&journal, &publication_ops(&writes, &deletes)).unwrap();
    writes[0].commit().unwrap();
    let _leaked = std::mem::ManuallyDrop::new(writes);
    let _leaked = std::mem::ManuallyDrop::new(deletes);

    // The skew the journal exists to repair: global-new, workspace-old.
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-new");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-old");
    assert!(journal.exists());

    // The next writer rolls the journal forward before observing the tree.
    drop(acquire_config_write_lock(&config).unwrap());

    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-new");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-new");
    assert!(!journal.exists(), "journal must be removed after recovery");
    assert!(staged_leftovers(temp.path()).is_empty());
    assert!(staged_leftovers(&temp.path().join("workspaces")).is_empty());
}

#[test]
fn recovery_consumes_staged_leftovers_without_litter() {
    let (temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let deletes = Vec::new();
    write_publication_journal(&journal, &publication_ops(&writes, &deletes)).unwrap();
    let _leaked = std::mem::ManuallyDrop::new(writes);
    let _leaked = std::mem::ManuallyDrop::new(deletes);

    recover_publication_journal(&journal).unwrap();

    assert!(!journal.exists());
    assert!(staged_leftovers(temp.path()).is_empty());
    assert!(staged_leftovers(&temp.path().join("workspaces")).is_empty());
}

#[test]
fn failed_commit_restores_originals_and_clears_journal() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let mut deletes = Vec::new();
    // Deterministic rename failure: the second staged file vanishes before
    // commit, so the first rename must be rolled back in-process.
    std::fs::remove_file(&writes[1].tmp).unwrap();

    let _error = commit_staged_config(&journal, &mut writes, &mut deletes).unwrap_err();

    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-old");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-old");
    assert!(!journal.exists(), "journal must be removed after abort");
}

#[test]
fn crash_during_abort_completes_restores_on_recovery() {
    let (_temp, config, workspace) = staged_config_tree();
    std::fs::write(&config, "global-new").unwrap();
    std::fs::write(&workspace, "ws-new").unwrap();
    let journal = publication_journal_path(&config);

    // Simulate `kill -9` halfway through applying an abort journal: the
    // first restore landed, the second staged file is still pending.
    let mut restores = vec![
        stage_atomic_write(&config, "global-old").unwrap(),
        stage_atomic_write(&workspace, "ws-old").unwrap(),
    ];
    let deletes = Vec::new();
    write_publication_journal(&journal, &publication_ops(&restores, &deletes)).unwrap();
    restores[0].commit().unwrap();
    let _leaked = std::mem::ManuallyDrop::new(restores);

    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-old");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-new");

    recover_publication_journal(&journal).unwrap();

    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-old");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-old");
    assert!(!journal.exists());
}

#[test]
fn recovery_skips_already_applied_ops() {
    let (_temp, config, _workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);

    // Crash after the last rename but before journal removal: every tmp is
    // gone while every target holds new bytes. Recovery must converge
    // without touching the targets.
    let staged = stage_atomic_write(&config, "global-new").unwrap();
    let target = staged.target.clone();
    let mut staged = staged;
    staged.commit().unwrap();
    let missing_tmp = target.with_file_name("config.toml.tmp.1.1");
    assert!(!missing_tmp.exists());
    let ops = vec![PublicationOp::Write {
        target: target.clone(),
        tmp: missing_tmp,
    }];
    write_publication_journal(&journal, &ops).unwrap();

    recover_publication_journal(&journal).unwrap();

    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-new");
    assert!(!journal.exists());
}

#[test]
fn corrupt_journal_fails_write_lock_closed() {
    let (_temp, config, _workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    std::fs::write(&journal, "{ not json").unwrap();

    let error = acquire_config_write_lock(&config).unwrap_err();
    assert!(error.to_string().contains("malformed"), "{error}");
    assert!(
        journal.exists(),
        "corrupt journal must be kept for forensics"
    );

    std::fs::write(&journal, r#"{"version":999,"ops":[]}"#).unwrap();
    let error = acquire_config_write_lock(&config).unwrap_err();
    assert!(error.to_string().contains("unsupported version"), "{error}");
}

#[test]
fn recovery_with_lost_write_fails_closed() {
    let (temp, config, _workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let missing_target = temp.path().join("vanished.toml");
    let missing_tmp = temp.path().join("vanished.toml.tmp.1.1");
    assert!(!missing_target.exists());
    assert!(!missing_tmp.exists());
    write_publication_journal(
        &journal,
        &[PublicationOp::Write {
            target: missing_target.clone(),
            tmp: missing_tmp,
        }],
    )
    .unwrap();

    let error = recover_publication_journal(&journal).unwrap_err();
    assert!(error.to_string().contains("is gone"), "{error}");
    assert!(journal.exists(), "failed recovery must keep the journal");
}

#[test]
fn empty_commit_writes_no_journal() {
    let (_temp, config, _workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    commit_staged_config(&journal, &mut [], &mut []).unwrap();
    assert!(!journal.exists());
}

#[test]
fn successful_commit_leaves_no_journal() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let mut deletes = Vec::new();
    commit_staged_config(&journal, &mut writes, &mut deletes).unwrap();
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-new");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-new");
    assert!(!journal.exists());
}
