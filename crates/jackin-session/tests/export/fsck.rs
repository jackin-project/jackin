// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_session::ExportError;
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};

use super::support::{branch_with_commit, export, exports, ref_of, snapshot, staging_left};

fn raw_id(hex: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    (0..hex.len())
        .step_by(2)
        .map(|at| Ok(u8::from_str_radix(&hex[at..at + 2], 16)?))
        .collect()
}

async fn evil_branch(handle: &SandboxHandle, entries: &[&str]) -> Result<(), Box<dyn Error>> {
    let format = handle.git(&["rev-parse", "--show-object-format"]).await?;
    assert_eq!(format.trim(), "sha1");
    let blob = handle.root().join("blob.txt");
    std::fs::write(&blob, b"content\n")?;
    let id = raw_id(
        handle
            .git(&["hash-object", "-w", &blob.display().to_string()])
            .await?
            .trim(),
    )?;
    let mut tree = Vec::new();
    for entry in entries {
        tree.extend_from_slice(format!("100644 {entry}\0").as_bytes());
        tree.extend_from_slice(&id);
    }
    let path = handle.root().join("tree.bin");
    std::fs::write(&path, tree)?;
    let tree_id = handle
        .git(&[
            "hash-object",
            "-t",
            "tree",
            "-w",
            "--literally",
            &path.display().to_string(),
        ])
        .await?;
    let commit = handle
        .git(&["commit-tree", tree_id.trim(), "-p", "HEAD", "-m", "evil"])
        .await?;
    handle
        .git(&["update-ref", "refs/heads/evil1", commit.trim()])
        .await?;
    Ok(())
}

async fn plain_fetch_accepts_it(handle: &SandboxHandle) -> TestResult {
    let bundle = handle.root().join("evil.bundle");
    let bundle_text = bundle.display().to_string();
    handle
        .git(&[
            "bundle",
            "create",
            "--quiet",
            &bundle_text,
            "refs/heads/evil1",
        ])
        .await?;
    let scratch = tempfile::tempdir()?;
    let scratch_text = scratch.path().display().to_string();
    handle.git(&["init", "-q", &scratch_text]).await?;
    handle
        .git(&[
            "-C",
            &scratch_text,
            "fetch",
            "-q",
            &bundle_text,
            "refs/heads/evil1:refs/heads/probe",
        ])
        .await?;
    let landed = handle
        .git(&[
            "-C",
            &scratch_text,
            "rev-parse",
            "--verify",
            "refs/heads/probe",
        ])
        .await?;
    assert_eq!(landed.trim().len(), 40);
    Ok(())
}

#[tokio::test]
async fn a_bundle_with_a_malformed_tree_is_refused_by_the_fetch_and_writes_no_ref() -> TestResult {
    guard(async {
        for (label, entries) in [
            ("a .git entry", vec![".git"]),
            ("duplicate entries", vec!["a", "a"]),
        ] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            evil_branch(&handle, &entries).await?;
            plain_fetch_accepts_it(&handle).await?;
            let before = snapshot(&rig).await?;

            let refused = export(&rig, &name).await;

            let Err(ExportError::HostGit { output, .. }) = refused else {
                return Err(
                    format!("{label}: expected a host git refusal, saw {refused:?}").into(),
                );
            };
            assert!(output.as_str().contains("fsck"), "{label}: {output:?}");
            let after = snapshot(&rig).await?;
            assert!(exports(&after).is_empty(), "{label}");
            assert_eq!(after.rest, before.rest, "{label}");
            assert_eq!(staging_left(&rig)?, Vec::<String>::new(), "{label}");

            handle.git(&["branch", "-q", "-D", "evil1"]).await?;
            branch_with_commit(&handle, "clean", "c.txt").await?;
            export(&rig, &name).await?;
            assert!(exports(&snapshot(&rig).await?).contains_key(&ref_of(&name, "clean")));
        }
        Ok(())
    })
    .await
}
