// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{CommitId, TerminalText};
use jackin_session::ExportError;
use jackin_testkit::{Rig, SandboxHandle, TestResult, default_limits, guard};
use tokio_util::sync::CancellationToken;

use super::support::{
    branch_with_commit, bundles_left, export, exports, ref_of, snapshot, staging_left,
};
use super::tap::Tap;

struct Case {
    label: &'static str,
    refs: Vec<&'static str>,
    extra: Vec<&'static str>,
    missing: Vec<&'static str>,
}

pub(crate) async fn crafted(
    handle: &SandboxHandle,
    refs: &[&str],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let path = handle.root().join("crafted.bundle");
    let mut args = vec![
        "bundle".to_owned(),
        "create".to_owned(),
        "--quiet".to_owned(),
        path.display().to_string(),
    ];
    args.extend(refs.iter().map(|name| (*name).to_owned()));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    handle.git(&args).await?;
    let bytes = std::fs::read(&path)?;
    std::fs::remove_file(path)?;
    Ok(bytes)
}

#[tokio::test]
async fn a_bundle_whose_heads_differ_from_the_requested_set_is_refused_before_any_ref_is_written()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        branch_with_commit(&handle, "work", "w.txt").await?;
        let second = branch_with_commit(&handle, "work2", "w2.txt").await?;
        handle.git(&["checkout", "-q", "main"]).await?;
        handle.git(&["tag", "t"]).await?;
        handle.git(&["branch", "jackin/other/x"]).await?;
        let cases = [
            Case {
                label: "an unchanged branch",
                refs: vec!["refs/heads/main", "refs/heads/work", "refs/heads/work2"],
                extra: vec!["refs/heads/main"],
                missing: vec![],
            },
            Case {
                label: "a tag",
                refs: vec!["refs/tags/t", "refs/heads/work", "refs/heads/work2"],
                extra: vec!["refs/tags/t"],
                missing: vec![],
            },
            Case {
                label: "HEAD",
                refs: vec!["HEAD", "refs/heads/work", "refs/heads/work2"],
                extra: vec!["HEAD"],
                missing: vec![],
            },
            Case {
                label: "a branch in the jackin namespace",
                refs: vec![
                    "refs/heads/jackin/other/x",
                    "refs/heads/work",
                    "refs/heads/work2",
                ],
                extra: vec!["refs/heads/jackin/other/x"],
                missing: vec![],
            },
            Case {
                label: "a missing head",
                refs: vec!["refs/heads/work"],
                extra: vec![],
                missing: vec!["refs/heads/work2"],
            },
        ];
        let mut bundles = Vec::new();
        for case in &cases {
            bundles.push(crafted(&handle, &case.refs).await?);
        }
        handle
            .git(&["branch", "-q", "-D", "jackin/other/x"])
            .await?;
        let tap = Tap::new(&rig);
        let session = tap.session(&rig, default_limits());
        let before = snapshot(&rig).await?;
        for (bytes, case) in bundles.into_iter().zip(&cases) {
            let label = case.label;
            tap.replace_next_download(bytes);

            let refused = session
                .export(&rig.config, &name, CancellationToken::new())
                .await;

            let Err(ExportError::BundleHeads {
                extra: found_extra,
                missing: found_missing,
            }) = refused
            else {
                return Err(format!("{label}: expected a refusal, saw {refused:?}").into());
            };
            let wanted_extra: Vec<TerminalText> = case
                .extra
                .iter()
                .map(|head| TerminalText::sanitize(head.as_bytes()))
                .collect();
            assert_eq!(found_extra, wanted_extra, "{label}");
            assert_eq!(found_missing, case.missing, "{label}");
            let after = snapshot(&rig).await?;
            assert!(exports(&after).is_empty(), "{label}");
            assert_eq!(after.rest, before.rest, "{label}");
            assert_eq!(staging_left(&rig)?, Vec::<String>::new(), "{label}");
            assert_eq!(bundles_left(&handle), Vec::<String>::new(), "{label}");
        }

        export(&rig, &name).await?;
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(landed.len(), 2);
        assert_eq!(
            landed.get(&ref_of(&name, "work2")).map(String::as_str),
            Some(second.as_str())
        );
        Ok(())
    })
    .await
}

async fn with_loose_branches(
    count: usize,
) -> Result<(Rig, SandboxHandle, CommitId), Box<dyn Error>> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    let head = CommitId::parse(handle.git(&["rev-parse", "HEAD"]).await?.trim())?;
    let heads = handle.root().join("project/.git/refs/heads");
    for index in 0..count {
        std::fs::write(heads.join(format!("b{index:04}")), format!("{head}\n"))?;
    }
    Ok((rig, handle, head))
}

#[tokio::test]
async fn export_takes_at_most_1024_heads_and_refuses_more_without_writing_anything() -> TestResult {
    guard(async {
        let (rig, handle, head) = with_loose_branches(1024).await?;
        let name = handle.name();
        let report = export(&rig, &name).await?;
        assert_eq!(report.branches.len(), 1024);
        let landed = exports(&snapshot(&rig).await?);
        assert_eq!(landed.len(), 1024);
        assert!(landed.values().all(|tip| tip == head.as_str()));

        let (rig, handle, _) = with_loose_branches(1025).await?;
        let name = handle.name();
        let before = snapshot(&rig).await?;
        let refused = export(&rig, &name).await;
        assert!(
            matches!(
                refused,
                Err(ExportError::TooManyHeads {
                    count: 1025,
                    limit: 1024
                })
            ),
            "{:?}",
            refused.as_ref().map(|report| report.branches.len())
        );
        let after = snapshot(&rig).await?;
        assert!(exports(&after).is_empty());
        assert_eq!(after.rest, before.rest);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&handle), Vec::<String>::new());
        Ok(())
    })
    .await
}
