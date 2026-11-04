// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use jackin_core::BranchName;
use jackin_openshell::{GatewayOp, TransferOp};
use jackin_session::{ExportError, PurgeError, PurgeMode, Unexported, UnknownReason, WorkState};
use jackin_testkit::{Fault, Rig, SandboxHandle, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{bundles_left, digests, export, exports, ref_of, snapshot, state, tip};

async fn add_detached(handle: &SandboxHandle, relative: &str) -> Result<String, Box<dyn Error>> {
    let path = handle.root().join(relative).display().to_string();
    handle
        .git(&["worktree", "add", "-q", "--detach", &path])
        .await?;
    Ok(std::fs::canonicalize(path)?.display().to_string())
}

async fn commit_in(
    handle: &SandboxHandle,
    worktree: &str,
    file: &str,
) -> Result<String, Box<dyn Error>> {
    std::fs::write(Path::new(worktree).join(file), file.as_bytes())?;
    handle.git(&["-C", worktree, "add", "-A"]).await?;
    handle
        .git(&["-C", worktree, "commit", "-q", "-m", file])
        .await?;
    Ok(handle
        .git(&["-C", worktree, "rev-parse", "HEAD"])
        .await?
        .trim()
        .to_owned())
}

fn dirty() -> WorkState {
    WorkState::Unexported(Unexported {
        uncommitted: 1,
        branches: Vec::new(),
        detached: None,
        worktrees: Vec::new(),
        unusable: Vec::new(),
    })
}

#[tokio::test]
async fn work_in_any_linked_worktree_counts_and_a_missing_worktree_does_not() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.git(&["branch", "side"]).await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let w2 = add_detached(&handle, "w2").await?;
        let w4 = handle.root().join("w4").display().to_string();
        handle.git(&["worktree", "add", "-q", &w4, "side"]).await?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);
        let quiet = export(&rig, &name).await?;
        assert!(quiet.bundle.is_none() && quiet.worktrees.is_empty());

        std::fs::write(Path::new(&w2).join("new.txt"), b"u")?;
        assert_eq!(state(&rig, &name).await, dirty());
        std::fs::remove_file(Path::new(&w2).join("new.txt"))?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        std::fs::create_dir_all(Path::new(&w2).join("target"))?;
        std::fs::write(Path::new(&w2).join("target/built.bin"), b"built")?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        std::fs::write(Path::new(&w4).join("README.md"), b"edited\n")?;
        assert_eq!(state(&rig, &name).await, dirty());
        let files = digests(&handle);
        let before = snapshot(&rig).await?;
        let refused = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await;
        assert!(
            matches!(&refused, Err(PurgeError::WorkNotExported { state, .. }) if **state == dirty()),
            "{refused:?}"
        );
        assert!(rig.world.gateway.sandbox(&name).is_some());
        assert_eq!(digests(&handle), files);
        assert_eq!(snapshot(&rig).await?.rest, before.rest);

        handle
            .git(&["-C", &w4, "checkout", "-q", "--", "README.md"])
            .await?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        std::fs::remove_dir_all(&w2)?;
        let listed = handle.git(&["worktree", "list", "--porcelain"]).await?;
        assert!(listed.contains("prunable"), "{listed}");
        let after_removal = state(&rig, &name).await;
        assert_ne!(
            after_removal,
            WorkState::Unknown(UnknownReason::InspectionFailed)
        );
        assert_eq!(after_removal, WorkState::Exported);
        export(&rig, &name).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_detached_worktree_whose_directory_moved_keeps_its_unique_commit_in_view() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let moved = add_detached(&handle, "w2").await?;
        add_detached(&handle, "w3").await?;
        let unique = commit_in(&handle, &moved, "only-here.txt").await?;
        let ids = worktree_ids(&handle)?;
        let id_of = |path: &str| -> Result<String, Box<dyn Error>> {
            ids.iter()
                .find(|(_, at)| at.as_str() == path)
                .map(|(id, _)| id.clone())
                .ok_or_else(|| "no worktree id".into())
        };
        let id = id_of(&moved)?;
        let wanted = unexported(&id)?;
        assert_eq!(state(&rig, &name).await, wanted);

        std::fs::rename(&moved, handle.root().join("elsewhere"))?;
        let listed = handle.git(&["worktree", "list", "--porcelain"]).await?;
        assert!(listed.contains("prunable"), "{listed}");
        assert_eq!(state(&rig, &name).await, wanted);
        let before = snapshot(&rig).await?;
        let refused = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await;
        assert!(
            matches!(&refused, Err(PurgeError::WorkNotExported { state, .. }) if **state == wanted),
            "{refused:?}"
        );
        assert!(rig.world.gateway.sandbox(&name).is_some());
        assert_eq!(snapshot(&rig).await?.rest, before.rest);

        let report = export(&rig, &name).await?;
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(
            landed,
            BTreeMap::from([(ref_of(&name, &format!("worktree/{id}")), unique.clone())])
        );
        assert_eq!(report.worktrees.len(), 1);
        assert_eq!(state(&rig, &name).await, WorkState::Exported);
        rig.world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await?;
        assert!(rig.world.gateway.sandbox(&name).is_none());
        Ok(())
    })
    .await
}

fn unexported(id: &str) -> Result<WorkState, Box<dyn Error>> {
    Ok(WorkState::Unexported(Unexported {
        uncommitted: 0,
        branches: Vec::new(),
        detached: None,
        worktrees: vec![BranchName::parse(id)?],
        unusable: Vec::new(),
    }))
}

fn worktree_ids(handle: &SandboxHandle) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut found = BTreeMap::new();
    for entry in std::fs::read_dir(handle.root().join("project/.git/worktrees"))? {
        let entry = entry?;
        let gitdir = std::fs::read_to_string(entry.path().join("gitdir"))?;
        let path = gitdir
            .trim_end()
            .strip_suffix("/.git")
            .ok_or("the gitdir file does not end in /.git")?
            .to_owned();
        found.insert(entry.file_name().to_string_lossy().into_owned(), path);
    }
    Ok(found)
}

async fn sandbox_repository(
    handle: &SandboxHandle,
) -> Result<(String, Vec<String>), Box<dyn Error>> {
    let refs = handle.git(&["for-each-ref"]).await?;
    let mut tree = Vec::new();
    let root = handle.root().join("project/.git/worktrees");
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                tree.push(format!(
                    "{} {:?}",
                    path.strip_prefix(&root)?.display(),
                    std::fs::read(&path)?
                ));
            }
        }
    }
    tree.sort();
    Ok((refs, tree))
}

#[tokio::test]
async fn a_detached_head_in_a_linked_worktree_is_exported_under_the_unique_worktree_name()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.git(&["branch", "side"]).await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let first = add_detached(&handle, "w2").await?;
        let second = add_detached(&handle, "d1/w").await?;
        let third = add_detached(&handle, "d2/w").await?;
        let fresh = add_detached(&handle, "fresh").await?;
        let on_branch = handle.root().join("w4").display().to_string();
        handle
            .git(&["worktree", "add", "-q", &on_branch, "side"])
            .await?;
        let on_branch = std::fs::canonicalize(on_branch)?.display().to_string();
        let committed = [
            commit_in(&handle, &first, "one.txt").await?,
            commit_in(&handle, &second, "two.txt").await?,
            commit_in(&handle, &third, "three.txt").await?,
        ];
        let side_tip = commit_in(&handle, &on_branch, "side.txt").await?;
        let main_tip = handle.commit_in_project("m.txt", b"m", "main").await?;
        let ids = worktree_ids(&handle)?;
        assert_eq!(ids.len(), 5);
        let by_path: BTreeMap<&str, &str> = ids
            .iter()
            .map(|(id, path)| (path.as_str(), id.as_str()))
            .collect();
        let mut expected = BTreeMap::from([(ref_of(&name, "main"), main_tip.as_str().to_owned())]);
        for (path, tip) in [&first, &second, &third].into_iter().zip(&committed) {
            let id = by_path.get(path.as_str()).ok_or("no worktree id")?;
            expected.insert(ref_of(&name, &format!("worktree/{id}")), tip.clone());
        }
        let side_id = by_path
            .get(on_branch.as_str())
            .ok_or("no id for the worktree on side")?;
        expected.insert(ref_of(&name, "side"), side_tip.clone());
        assert_eq!(expected.len(), 5);
        assert_ne!(by_path.get(second.as_str()), by_path.get(third.as_str()));
        assert!(by_path.contains_key(fresh.as_str()));
        let sandbox_before = sandbox_repository(&handle).await?;
        let before = snapshot(&rig).await?;

        let report = export(&rig, &name).await?;

        let landed = exports(&snapshot(&rig).await?);
        assert!(
            !landed.contains_key(&ref_of(&name, &format!("worktree/{side_id}"))),
            "the worktree on the branch side is exported as a detached head: {landed:?}"
        );
        assert_eq!(landed, expected);
        assert_eq!(snapshot(&rig).await?.rest, before.rest);
        let reported: BTreeMap<String, String> = report
            .worktrees
            .iter()
            .map(|item| (item.host_ref.clone(), item.tip.as_str().to_owned()))
            .collect();
        expected.remove(&ref_of(&name, "main"));
        expected.remove(&ref_of(&name, "side"));
        assert_eq!(reported, expected);
        assert_eq!(report.branches.len(), 2);
        assert_eq!(state(&rig, &name).await, WorkState::Exported);
        assert_eq!(sandbox_repository(&handle).await?, sandbox_before);
        assert_eq!(bundles_left(&handle), Vec::<String>::new());

        let again = export(&rig, &name).await?;
        assert_eq!(again.worktrees, report.worktrees);
        assert_eq!(sandbox_repository(&handle).await?, sandbox_before);

        let token = CancellationToken::new();
        rig.world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Download),
            token: token.clone(),
        });
        let cancelled = rig.world.session.export(&rig.config, &name, token).await;
        assert!(
            matches!(cancelled, Err(ExportError::Cancelled { group: None })),
            "{cancelled:?}"
        );
        assert_eq!(sandbox_repository(&handle).await?, sandbox_before);
        rig.world.gateway.inject(Fault::CorruptDownload);
        let corrupt = export(&rig, &name).await;
        assert!(
            matches!(
                corrupt,
                Err(ExportError::Gateway(
                    jackin_openshell::GatewayError::Transfer {
                        op: TransferOp::Digest,
                        ..
                    }
                ))
            ),
            "{corrupt:?}"
        );
        assert_eq!(sandbox_repository(&handle).await?, sandbox_before);
        assert_eq!(bundles_left(&handle), Vec::<String>::new());
        Ok(())
    })
    .await
}

async fn colliding_branch_and_worktree(branch: &str) -> TestResult {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    let name = handle.name();
    handle.commit_in_project("m.txt", b"m", "main").await?;
    handle.git(&["branch", "other"]).await?;
    handle.git(&["branch", branch]).await?;
    add_detached(&handle, "w2").await?;
    let before = snapshot(&rig).await?;

    let refused = export(&rig, &name).await;

    assert!(
        matches!(
            refused,
            Err(ExportError::RefConflict { .. } | ExportError::UnusableBranch { .. })
        ),
        "{branch}: {refused:?}"
    );
    let after = snapshot(&rig).await?;
    assert!(exports(&after).is_empty(), "{branch}");
    assert_eq!(after.rest, before.rest);
    assert_eq!(bundles_left(&handle), Vec::<String>::new());

    handle.git(&["branch", "-q", "-D", branch]).await?;
    export(&rig, &name).await?;
    let landed = exports(&snapshot(&rig).await?);
    let names: Vec<&String> = landed.keys().collect();
    assert_eq!(
        names,
        vec![
            &ref_of(&name, "main"),
            &ref_of(&name, "other"),
            &ref_of(&name, "worktree/w2")
        ],
        "{branch}"
    );
    assert_eq!(
        landed
            .get(&ref_of(&name, "worktree/w2"))
            .map(String::as_str),
        Some(tip(&handle, "HEAD").await?.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn targets_that_collide_or_nest_are_refused_before_any_import_and_a_retry_exports_all()
-> TestResult {
    guard(async {
        colliding_branch_and_worktree("worktree/w2").await?;
        colliding_branch_and_worktree("worktree").await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_linked_worktree_whose_head_is_symbolic_outside_heads_is_unexported_work() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let linked = add_detached(&handle, "w2").await?;
        handle
            .git(&["-C", &linked, "update-ref", "refs/remotes/origin/y", "HEAD"])
            .await?;
        handle
            .git(&[
                "-C",
                &linked,
                "symbolic-ref",
                "HEAD",
                "refs/remotes/origin/y",
            ])
            .await?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        let commit = commit_in(&handle, &linked, "w.txt").await?;

        let found = state(&rig, &name).await;
        let WorkState::Unexported(found) = found else {
            return Err(format!("{found:?}").into());
        };
        assert_eq!(found.worktrees.len(), 1, "{found:?}");
        let purge = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await;
        assert!(
            matches!(purge, Err(PurgeError::WorkNotExported { .. })),
            "{purge:?}"
        );

        let report = export(&rig, &name).await?;

        let tips: Vec<&str> = report
            .worktrees
            .iter()
            .map(|worktree| worktree.tip.as_str())
            .collect();
        assert_eq!(tips, [commit.as_str()]);
        assert_eq!(state(&rig, &name).await, WorkState::Exported);
        Ok(())
    })
    .await
}
