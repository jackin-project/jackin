// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::CommitId;
use jackin_session::ExportError;
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};

use super::support::{
    self, branch_with_commit, bundles_left, export, exports, ref_of, snapshot, tip,
};

fn rewrite_packed(handle: &SandboxHandle, add: &[(&str, &CommitId)], drop: &[&str]) -> TestResult {
    let named = |name: &str| format!("refs/heads/{name}").into_bytes();
    let add: Vec<(Vec<u8>, &CommitId)> = add.iter().map(|(name, id)| (named(name), *id)).collect();
    let drop: Vec<Vec<u8>> = drop.iter().map(|name| named(name)).collect();
    support::rewrite_packed(&handle.root().join("project/.git"), &add, &drop)
}

const FOLDED: [(&str, &str); 5] = [
    ("Feature", "feature"),
    ("stra\u{df}e", "strasse"),
    ("\u{17f}ix", "six"),
    ("\u{b5}x", "\u{3bc}x"),
    ("\u{3c3}", "\u{3c2}"),
];
const NORMALIZED: (&str, &str) = ("caf\u{e9}", "cafe\u{301}");

#[tokio::test]
async fn sandbox_branches_that_differ_only_by_case_or_normalization_are_refused_before_any_ref_is_written()
-> TestResult {
    guard(async {
        for (first, second) in FOLDED.into_iter().chain([NORMALIZED]) {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            let work = handle.commit_in_project("a.txt", b"a", "work").await?;
            handle.git(&["pack-refs", "--all", "--prune"]).await?;
            rewrite_packed(&handle, &[(first, &work), (second, &work)], &[])?;
            let before = snapshot(&rig).await?;

            let refused = export(&rig, &name).await;

            assert!(
                matches!(refused, Err(ExportError::RefConflict { .. })),
                "{first:?} and {second:?}: {refused:?}"
            );
            let after = snapshot(&rig).await?;
            assert!(exports(&after).is_empty(), "{first:?}");
            assert_eq!(after.rest, before.rest);
            assert_eq!(bundles_left(&handle), Vec::<String>::new());

            rewrite_packed(&handle, &[], &[second])?;
            export(&rig, &name).await?;
            let landed = exports(&snapshot(&rig).await?);
            assert!(landed.contains_key(&ref_of(&name, first)), "{first:?}");
            assert!(!landed.contains_key(&ref_of(&name, second)), "{second:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_branch_that_aliases_an_exported_host_ref_is_refused_until_that_ref_is_deleted()
-> TestResult {
    guard(async {
        for (old, new) in FOLDED {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            branch_with_commit(&handle, old, "f.txt").await?;
            handle.git(&["checkout", "-q", "main"]).await?;
            export(&rig, &name).await?;
            handle.git(&["branch", "-q", "-D", old]).await?;
            branch_with_commit(&handle, new, "g.txt").await?;
            handle.git(&["checkout", "-q", "main"]).await?;
            let renamed = tip(&handle, new).await?;
            let before = snapshot(&rig).await?;

            let refused = export(&rig, &name).await;

            let Err(ExportError::RefConflict { host_ref, .. }) = refused else {
                return Err(
                    format!("{old:?} then {new:?}: expected a conflict, saw {refused:?}").into(),
                );
            };
            assert_eq!(host_ref, ref_of(&name, new));
            let after = snapshot(&rig).await?;
            assert_eq!(exports(&after), exports(&before), "{old:?}");
            assert_eq!(after.rest, before.rest, "{old:?}");

            rig.repo
                .git(&["update-ref", "-d", &ref_of(&name, old)])
                .await?;
            export(&rig, &name).await?;
            let landed = exports(&snapshot(&rig).await?);
            assert_eq!(
                landed.get(&ref_of(&name, new)).map(String::as_str),
                Some(renamed.as_str()),
                "{old:?}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unborn_worktree_branch_that_aliases_a_target_ref_blocks_the_export() -> TestResult {
    guard(async {
        for (wanted, held) in FOLDED.into_iter().chain([NORMALIZED]) {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            branch_with_commit(&handle, wanted, "f.txt").await?;
            handle.git(&["checkout", "-q", "main"]).await?;
            let other = tempfile::tempdir()?;
            let path = other.path().join("tree").to_string_lossy().into_owned();
            rig.repo
                .git(&["worktree", "add", "-q", "--detach", &path])
                .await?;
            let registered = rig.repo.root().as_path().join(".git/worktrees");
            let entry = std::fs::read_dir(&registered)?
                .next()
                .ok_or("no registered worktree")??;
            std::fs::write(
                entry.path().join("HEAD"),
                format!("ref: {}\n", ref_of(&name, held)),
            )?;
            let before = snapshot(&rig).await?;

            let refused = export(&rig, &name).await;

            let Err(ExportError::RefCheckedOut { host_ref, .. }) = refused else {
                return Err(format!(
                    "{wanted:?} against {held:?}: expected a refusal, saw {refused:?}"
                )
                .into());
            };
            assert_eq!(host_ref, ref_of(&name, wanted));
            let after = snapshot(&rig).await?;
            assert_eq!(exports(&after), exports(&before), "{wanted:?}");
        }
        Ok(())
    })
    .await
}
