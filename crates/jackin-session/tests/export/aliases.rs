// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::CommitId;
use jackin_session::ExportError;
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};

use super::support::{branch_with_commit, bundles_left, export, exports, ref_of, snapshot, tip};

const HEADER: &[u8] = b"# pack-refs with: peeled fully-peeled sorted \n";

fn rewrite_packed(handle: &SandboxHandle, add: &[(&str, &CommitId)], drop: &[&str]) -> TestResult {
    let file = handle.root().join("project/.git/packed-refs");
    let mut entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for line in std::fs::read(&file)?.split(|byte| *byte == b'\n') {
        if line.is_empty() || line.starts_with(b"#") || line.starts_with(b"^") {
            continue;
        }
        let split = line
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or("bad line")?;
        entries.push((line[split + 1..].to_vec(), line[..split].to_vec()));
    }
    entries.retain(|(name, _)| {
        !drop
            .iter()
            .any(|gone| name == format!("refs/heads/{gone}").as_bytes())
    });
    for (name, id) in add {
        entries.push((
            format!("refs/heads/{name}").into_bytes(),
            id.as_str().as_bytes().to_vec(),
        ));
    }
    entries.sort();
    let mut out = HEADER.to_vec();
    for (name, id) in entries {
        out.extend(id);
        out.push(b' ');
        out.extend(name);
        out.push(b'\n');
    }
    std::fs::write(&file, out)?;
    Ok(())
}

#[tokio::test]
async fn sandbox_branches_that_differ_only_by_case_or_normalization_are_refused_before_any_ref_is_written()
-> TestResult {
    guard(async {
        for (first, second) in [("Feature", "feature"), ("caf\u{e9}", "cafe\u{301}")] {
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
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        branch_with_commit(&handle, "Feature", "f.txt").await?;
        handle.git(&["checkout", "-q", "main"]).await?;
        export(&rig, &name).await?;
        handle.git(&["branch", "-q", "-D", "Feature"]).await?;
        branch_with_commit(&handle, "feature", "g.txt").await?;
        handle.git(&["checkout", "-q", "main"]).await?;
        let renamed = tip(&handle, "feature").await?;
        let before = snapshot(&rig).await?;

        let refused = export(&rig, &name).await;

        let Err(ExportError::RefConflict { host_ref, .. }) = refused else {
            return Err(format!("expected a conflict, saw {refused:?}").into());
        };
        assert_eq!(host_ref, ref_of(&name, "feature"));
        let after = snapshot(&rig).await?;
        assert_eq!(exports(&after), exports(&before));
        assert_eq!(after.rest, before.rest);

        rig.repo
            .git(&["update-ref", "-d", &ref_of(&name, "Feature")])
            .await?;
        export(&rig, &name).await?;
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(
            landed.get(&ref_of(&name, "feature")).map(String::as_str),
            Some(renamed.as_str())
        );
        Ok(())
    })
    .await
}
