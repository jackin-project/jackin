// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        expected_sha256: Sha256::digest(b"global-new").into(),
        recovery_owned: false,
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
        expected_sha256: Sha256::digest(b"missing-new").into(),
        recovery_owned: false,
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

#[test]
fn abort_preparation_failure_retains_forward_generation_after_normal_drop() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let pending_tmp = writes[1].tmp.clone();
    let mut committed = 0;
    let error = commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        write_publication_ops,
        |write| {
            committed += 1;
            if committed == 2 {
                return Err(injected_failure());
            }
            write.commit()
        },
        |_, _| Err(injected_failure()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("rollback failed"));
    drop(writes);
    assert!(pending_tmp.is_file(), "forward journal owns pending bytes");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-new");
    assert_eq!(std::fs::read_to_string(&workspace).unwrap(), "ws-old");
    assert_recovered_generation(&config, &workspace, ["global-new", "ws-new"]);
}

#[test]
fn abort_journal_installation_failure_retains_forward_generation_after_normal_drop() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let mut publications = 0;
    let mut commits = 0;
    commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        |path, ops| {
            publications += 1;
            if publications == 2 {
                return Err(injected_failure());
            }
            write_publication_ops(path, ops)
        },
        |write| {
            commits += 1;
            if commits == 2 {
                return Err(injected_failure());
            }
            write.commit()
        },
        stage_atomic_write_bytes,
    )
    .unwrap_err();
    drop(writes);
    assert_recovered_generation(&config, &workspace, ["global-new", "ws-new"]);
}

#[test]
fn journal_installation_error_after_publication_retains_all_staged_bytes() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        |path, ops| {
            write_publication_ops(path, ops)?;
            Err(injected_failure())
        },
        StagedWrite::commit,
        stage_atomic_write_bytes,
    )
    .unwrap_err();
    drop(writes);
    assert_recovered_generation(&config, &workspace, ["global-new", "ws-new"]);
}

#[test]
fn abort_journal_installation_error_after_publication_retains_restore_bytes() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let mut publications = 0;
    let mut commits = 0;
    commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        |path, ops| {
            publications += 1;
            write_publication_ops(path, ops)?;
            if publications == 2 {
                return Err(injected_failure());
            }
            Ok(())
        },
        |write| {
            commits += 1;
            write.commit()?;
            if commits == 2 {
                return Err(injected_failure());
            }
            Ok(())
        },
        stage_atomic_write_bytes,
    )
    .unwrap_err();
    drop(writes);
    assert_recovered_generation(&config, &workspace, ["global-old", "ws-old"]);
}

#[test]
fn recovery_rejects_missing_staged_bytes_with_old_target() {
    let (_temp, config, _workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let writes = vec![stage_atomic_write(&config, "global-new").unwrap()];
    write_publication_journal(&journal, &writes, &[]).unwrap();
    std::fs::remove_file(&writes[0].tmp).unwrap();
    drop(writes);
    let error = acquire_config_write_lock(&config).unwrap_err();
    assert!(error.to_string().contains("generation mismatch"));
    assert!(journal.is_file());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-old");
}

#[test]
fn recovery_rejects_modified_staged_generation() {
    let (_temp, config, _workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![stage_atomic_write(&config, "global-new").unwrap()];
    write_publication_journal(&journal, &writes, &[]).unwrap();
    writes[0].recovery_owned = true;
    let tmp = writes[0].tmp.clone();
    std::fs::write(&tmp, "wrong-generation").unwrap();
    drop(writes);
    let error = acquire_config_write_lock(&config).unwrap_err();
    assert!(error.to_string().contains("generation mismatch"));
    assert!(journal.is_file());
    assert!(tmp.is_file());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "global-old");
}

#[test]
fn failed_restore_preparation_cleans_only_unjournaled_restore_files() {
    let (_temp, config, workspace) = staged_config_tree();
    let third = workspace.with_file_name("third.toml");
    std::fs::write(&third, "third-old").unwrap();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
        stage_atomic_write(&third, "third-new").unwrap(),
    ];
    let mut commits = 0;
    let mut restores = Vec::new();
    commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        write_publication_ops,
        |write| {
            commits += 1;
            if commits == 3 {
                return Err(injected_failure());
            }
            write.commit()
        },
        |target, contents| {
            if !restores.is_empty() {
                return Err(injected_failure());
            }
            let staged = stage_atomic_write_bytes(target, contents)?;
            restores.push(staged.tmp.clone());
            Ok(staged)
        },
    )
    .unwrap_err();
    assert_eq!(restores.len(), 1);
    assert!(
        !restores[0].exists(),
        "unjournaled restores remain locally owned"
    );
    assert!(writes[0].committed && writes[1].committed);
    drop(writes);
    assert_recovered_generation(&config, &workspace, ["global-new", "ws-new"]);
    assert_eq!(std::fs::read_to_string(third).unwrap(), "third-new");
}

#[test]
fn completed_abort_returns_unreferenced_forward_files_to_local_cleanup() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let pending_tmp = writes[1].tmp.clone();
    let mut commits = 0;
    commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        write_publication_ops,
        |write| {
            commits += 1;
            if commits == 2 {
                return Err(injected_failure());
            }
            write.commit()
        },
        stage_atomic_write_bytes,
    )
    .unwrap_err();
    drop(writes);
    assert!(!pending_tmp.exists());
    assert!(!journal.exists());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "global-old");
    assert_eq!(std::fs::read_to_string(workspace).unwrap(), "ws-old");
}

#[test]
fn journal_installation_error_before_publication_leaves_collectable_orphans() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let tmps: Vec<_> = writes.iter().map(|write| write.tmp.clone()).collect();
    commit_staged_config_with(
        &journal,
        &mut writes,
        &mut [],
        |_, _| Err(injected_failure()),
        StagedWrite::commit,
        stage_atomic_write_bytes,
    )
    .unwrap_err();
    drop(writes);
    assert!(!journal.exists());
    assert!(tmps.iter().all(|tmp| tmp.is_file()));
    assert_recovered_generation(&config, &workspace, ["global-old", "ws-old"]);
}
