// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn recovery_rejects_legacy_journal_without_generation_proof() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    std::fs::write(&journal, r#"{"version":1,"ops":[]}"#).unwrap();
    let error = acquire_config_write_lock(&config).unwrap_err();
    assert!(error.to_string().contains("unsupported version"));
    assert!(journal.is_file());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "global-old");
    assert_eq!(std::fs::read_to_string(workspace).unwrap(), "ws-old");
}

#[test]
fn abort_apply_failure_retains_restore_generation_after_normal_drop() {
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
                std::fs::remove_file(&workspace)?;
                std::fs::create_dir(&workspace)?;
                std::fs::write(workspace.join("blocker"), b"fixture blocker")?;
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
    assert!(journal.is_file());
    std::fs::remove_file(workspace.join("blocker")).unwrap();
    std::fs::remove_dir(&workspace).unwrap();
    std::fs::write(&workspace, "ws-new").unwrap();
    assert_recovered_generation(&config, &workspace, ["global-old", "ws-old"]);
}

#[test]
fn staged_parent_sync_precedes_both_forward_and_restore_journals() {
    use std::cell::{Cell, RefCell};
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let sibling_workspace = workspace.with_file_name("sibling.toml");
    std::fs::write(&sibling_workspace, "sibling-old").unwrap();
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
        stage_atomic_write(&sibling_workspace, "sibling-new").unwrap(),
    ];
    let synced = RefCell::new(HashSet::new());
    let publications = Cell::new(0);
    let mut commits = 0;
    commit_staged_config_with_sync(
        &journal,
        &mut writes,
        &mut [],
        (
            |path: &Path, ops: &[PublicationOp]| {
                let expected: HashSet<_> = ops
                    .iter()
                    .filter_map(|op| match op {
                        PublicationOp::Write { tmp, .. } => {
                            Some(tmp.parent().unwrap().to_path_buf())
                        }
                        PublicationOp::Delete { .. } => None,
                    })
                    .collect();
                assert_eq!(
                    *synced.borrow(),
                    expected,
                    "staged parent sync must precede each journal publication"
                );
                synced.borrow_mut().clear();
                publications.set(publications.get() + 1);
                write_publication_ops(path, ops)
            },
            |write: &mut StagedWrite| {
                commits += 1;
                write.commit()?;
                if commits == 2 {
                    return Err(injected_failure());
                }
                Ok(())
            },
            stage_atomic_write_bytes,
        ),
        |tmp: &Path| {
            sync_parent(tmp)?;
            assert!(
                synced
                    .borrow_mut()
                    .insert(tmp.parent().unwrap().to_path_buf()),
                "each parent sync occurs once per publication"
            );
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(publications.get(), 2);
    drop(writes);
    assert_recovered_generation(&config, &workspace, ["global-old", "ws-old"]);
    assert_eq!(
        std::fs::read_to_string(sibling_workspace).unwrap(),
        "sibling-old"
    );
}

#[test]
fn forward_staged_parent_sync_failure_prevents_publication_and_cleans_local_files() {
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let tmps: Vec<_> = writes.iter().map(|write| write.tmp.clone()).collect();
    let mut synced = 0;
    let error = commit_staged_config_with_sync(
        &journal,
        &mut writes,
        &mut [],
        (
            |_: &Path, _: &[PublicationOp]| -> crate::ConfigResult<()> {
                panic!("journal publication must not follow failed staged parent sync")
            },
            StagedWrite::commit,
            stage_atomic_write_bytes,
        ),
        |tmp: &Path| {
            synced += 1;
            if synced == 2 {
                return Err(std::io::Error::other("injected staged parent sync failure").into());
            }
            sync_parent(tmp)
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected staged parent sync failure")
    );
    assert_eq!(synced, 2);
    assert!(!journal.exists());
    drop(writes);
    assert!(tmps.iter().all(|tmp| !tmp.exists()));
    assert_recovered_generation(&config, &workspace, ["global-old", "ws-old"]);
    let mut retry = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    commit_staged_config(&journal, &mut retry, &mut []).unwrap();
    assert_recovered_generation(&config, &workspace, ["global-new", "ws-new"]);
}

#[test]
fn restore_staged_parent_sync_failure_retains_forward_journal_and_cleans_local_restores() {
    use std::cell::Cell;
    let (_temp, config, workspace) = staged_config_tree();
    let journal = publication_journal_path(&config);
    let mut writes = vec![
        stage_atomic_write(&config, "global-new").unwrap(),
        stage_atomic_write(&workspace, "ws-new").unwrap(),
    ];
    let pending_tmp = writes[1].tmp.clone();
    let publications = Cell::new(0);
    let mut commits = 0;
    let mut restores = Vec::new();
    let error = commit_staged_config_with_sync(
        &journal,
        &mut writes,
        &mut [],
        (
            |path: &Path, ops: &[PublicationOp]| {
                publications.set(publications.get() + 1);
                assert_eq!(
                    publications.get(),
                    1,
                    "failed restore parent sync must prevent abort publication"
                );
                write_publication_ops(path, ops)
            },
            |write: &mut StagedWrite| {
                commits += 1;
                if commits == 2 {
                    return Err(injected_failure());
                }
                write.commit()
            },
            |target: &Path, bytes: &[u8]| {
                let staged = stage_atomic_write_bytes(target, bytes)?;
                restores.push(staged.tmp.clone());
                Ok(staged)
            },
        ),
        |tmp: &Path| {
            if publications.get() == 1 {
                return Err(std::io::Error::other("injected restore parent sync failure").into());
            }
            sync_parent(tmp)
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected restore parent sync failure")
    );
    assert_eq!(publications.get(), 1);
    assert_eq!(restores.len(), 1);
    assert!(!restores[0].exists());
    assert!(journal.is_file());
    drop(writes);
    assert!(pending_tmp.is_file());
    assert_recovered_generation(&config, &workspace, ["global-new", "ws-new"]);
}
