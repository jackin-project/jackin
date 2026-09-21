// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{AppConfig, ConfigEditor};
use jackin_core::JackinPaths;
use std::sync::mpsc;

fn journal_workspace_toml(workdir: &str) -> String {
    format!(
        "version = \"{}\"\nworkdir = \"{workdir}\"\n\n[[mounts]]\nsrc = \"/host/source\"\ndst = \"{workdir}\"\n",
        crate::CURRENT_WORKSPACE_VERSION
    )
}

fn journal_global_with_marker(global_before: &str) -> String {
    let mut doc: toml_edit::DocumentMut = global_before.parse().unwrap();
    doc["env"]["JOURNAL_RECOVERED"] = toml_edit::value("yes");
    doc.to_string()
}

fn assert_no_staged_files(paths: &JackinPaths) {
    for dir in [&paths.config_dir, &paths.workspaces_dir] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.map(|entry| entry.unwrap()) {
            assert!(
                !entry.file_name().to_string_lossy().contains(".tmp."),
                "staged file leaked: {}",
                entry.path().display()
            );
        }
    }
}

#[test]
fn publication_journal_with_zero_completed_renames_recovers_on_open() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();

    // Simulate kill -9 after the journal fsync but before the first rename:
    // staged tmps plus journal on disk, zero renames completed.
    let global_before = std::fs::read_to_string(&paths.config_file).unwrap();
    let new_global = journal_global_with_marker(&global_before);
    let beta_path = paths.workspaces_dir.join("beta.toml");
    let beta_contents = journal_workspace_toml("/workspace/beta");
    let journal_path = publication_journal_path(&paths.config_file);
    let staged = vec![
        stage_atomic_write(&paths.config_file, &new_global).unwrap(),
        stage_atomic_write(&beta_path, &beta_contents).unwrap(),
    ];
    let deletes: Vec<StagedDelete> = Vec::new();
    write_publication_journal(&journal_path, &staged, &deletes).unwrap();
    leak_staged_writes(staged);
    assert!(journal_path.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&journal_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        global_before
    );

    drop(ConfigEditor::open(&paths).unwrap());

    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        new_global
    );
    assert_eq!(std::fs::read_to_string(&beta_path).unwrap(), beta_contents);
    assert!(!journal_path.exists());
    assert_no_staged_files(&paths);
    // Recovered tree passes full load-time validation.
    AppConfig::load_or_init(&paths).unwrap();
}

#[test]
fn publication_journal_first_rename_only_completes_writes_and_deletes() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let alpha_path = paths.workspaces_dir.join("alpha.toml");
    let beta_path = paths.workspaces_dir.join("beta.toml");
    std::fs::write(&alpha_path, journal_workspace_toml("/workspace/alpha")).unwrap();
    std::fs::write(&beta_path, journal_workspace_toml("/workspace/beta")).unwrap();

    // Simulate kill -9 after the first rename with a delete still pending:
    // global new, alpha staged, beta awaiting deletion.
    let global_before = std::fs::read_to_string(&paths.config_file).unwrap();
    let new_global = journal_global_with_marker(&global_before);
    let new_alpha = journal_workspace_toml("/workspace/alpha-new");
    let journal_path = publication_journal_path(&paths.config_file);
    let staged = vec![
        stage_atomic_write(&paths.config_file, &new_global).unwrap(),
        stage_atomic_write(&alpha_path, &new_alpha).unwrap(),
    ];
    let deletes = vec![stage_delete(&beta_path).unwrap().unwrap()];
    write_publication_journal(&journal_path, &staged, &deletes).unwrap();
    let global_tmp = staged.first().unwrap().tmp.clone();
    leak_staged_writes(staged);
    std::fs::rename(&global_tmp, &paths.config_file).unwrap();
    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        new_global
    );

    drop(ConfigEditor::open(&paths).unwrap());

    assert_eq!(std::fs::read_to_string(&alpha_path).unwrap(), new_alpha);
    assert!(!beta_path.exists());
    assert!(!journal_path.exists());
    assert_no_staged_files(&paths);
    AppConfig::load_or_init(&paths).unwrap();
}

#[test]
fn recover_pending_publication_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();

    let global_before = std::fs::read_to_string(&paths.config_file).unwrap();
    let new_global = journal_global_with_marker(&global_before);
    let beta_path = paths.workspaces_dir.join("beta.toml");
    let beta_contents = journal_workspace_toml("/workspace/beta");
    let journal_path = publication_journal_path(&paths.config_file);
    let staged = vec![
        stage_atomic_write(&paths.config_file, &new_global).unwrap(),
        stage_atomic_write(&beta_path, &beta_contents).unwrap(),
    ];
    let deletes: Vec<StagedDelete> = Vec::new();
    write_publication_journal(&journal_path, &staged, &deletes).unwrap();
    leak_staged_writes(staged);

    recover_pending_publication(&paths.config_file).unwrap();
    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        new_global
    );
    assert!(!journal_path.exists());

    // Second run sees no journal and changes nothing.
    recover_pending_publication(&paths.config_file).unwrap();
    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        new_global
    );
    assert_eq!(std::fs::read_to_string(&beta_path).unwrap(), beta_contents);
    AppConfig::load_or_init(&paths).unwrap();
}

#[test]
fn recover_pending_publication_collects_unlisted_staged_garbage() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

    let orphan = paths.config_file.with_file_name("config.toml.tmp.12345.7");
    let orphan_ws = paths.workspaces_dir.join("alpha.toml.tmp.12345.8");
    let keeper = paths.workspaces_dir.join("notes.tmp.1.2.toml");
    std::fs::write(&orphan, b"orphan").unwrap();
    std::fs::write(&orphan_ws, b"orphan").unwrap();
    std::fs::write(&keeper, b"operator").unwrap();

    recover_pending_publication(&paths.config_file).unwrap();

    assert!(!orphan.exists());
    assert!(!orphan_ws.exists());
    assert_eq!(std::fs::read(&keeper).unwrap(), b"operator".to_vec());
}

#[test]
fn recover_pending_publication_rejects_corrupt_journal() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();
    let journal_path = publication_journal_path(&paths.config_file);
    std::fs::write(&journal_path, b"not = [valid toml").unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    assert!(err.to_string().contains("corrupt"), "{err:#}");
    // Recovery never touches a journal it cannot parse.
    assert_eq!(
        std::fs::read(&journal_path).unwrap(),
        b"not = [valid toml".to_vec()
    );
}

#[test]
fn recover_pending_publication_rejects_unsupported_journal_version() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();
    let journal_path = publication_journal_path(&paths.config_file);
    std::fs::write(&journal_path, r#"{"version":999,"ops":[]}"#).unwrap();

    let err = ConfigEditor::open(&paths).unwrap_err();
    assert!(err.to_string().contains("unsupported"), "{err:#}");
    assert!(journal_path.exists());
}

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
    write_publication_journal(&journal, &writes, &deletes).unwrap();
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
    write_publication_journal(&journal, &writes, &deletes).unwrap();
    let _leaked = std::mem::ManuallyDrop::new(writes);
    let _leaked = std::mem::ManuallyDrop::new(deletes);

    recover_pending_publication(&config).unwrap();

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
    write_publication_journal(&journal, &restores, &deletes).unwrap();
    restores[0].commit().unwrap();
    let _leaked = std::mem::ManuallyDrop::new(restores);

    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-old");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-new");

    recover_pending_publication(&config).unwrap();

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
    let writes = vec![StagedWrite {
        target: target.clone(),
        tmp: missing_tmp,
        original: TargetState::Missing,
        committed: false,
    }];
    write_publication_journal(&journal, &writes, &[]).unwrap();

    recover_pending_publication(&config).unwrap();

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
    let writes = vec![StagedWrite {
        target: missing_target.clone(),
        tmp: missing_tmp,
        original: TargetState::Missing,
        committed: false,
    }];
    write_publication_journal(&journal, &writes, &[]).unwrap();

    let error = recover_pending_publication(&config).unwrap_err();
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
