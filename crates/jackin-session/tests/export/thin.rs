// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::process::Stdio;

use jackin_session::ExportError;
use jackin_testkit::{RepoFixture, Rig, TestResult, guard};
use tokio::io::AsyncWriteExt;

use super::lab::Lab;
use super::pack::header_len;
use super::support::{exports, snapshot, staging_left};

async fn git_with(
    dir: &Path,
    stdin: &[u8],
    args: &[&str],
) -> Result<(bool, Vec<u8>, String), Box<dyn std::error::Error>> {
    let mut child = tokio::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(stdin).await?;
    }
    let output = child.wait_with_output().await?;
    Ok((
        output.status.success(),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

async fn big_repo() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    let mut text = String::new();
    for line in 0..3000 {
        text.push_str(&format!("line {line} of the big file\n"));
    }
    repo.file("big.txt", text.as_bytes()).await?;
    repo.commit().await?;
    Ok(repo)
}

#[tokio::test]
async fn a_thin_pack_is_never_completed_from_host_objects() -> TestResult {
    guard(async {
        let rig = Rig::builder().repo(big_repo().await?).build().await?;
        let mut lab = Lab::over(rig).await?;
        let project = lab.handle.root().join("project");
        let mut changed = String::new();
        for line in 0..3000 {
            let word = if line == 1500 { "edited" } else { "big" };
            changed.push_str(&format!("line {line} of the {word} file\n"));
        }
        lab.handle
            .commit_in_project("big.txt", changed.as_bytes(), "edit")
            .await?;
        lab.advance().await?;
        let (ok, pack, error) = git_with(
            &project,
            b"refs/heads/work\n^refs/heads/main\n",
            &["pack-objects", "--revs", "--thin", "--stdout"],
        )
        .await?;
        assert!(ok, "{error}");
        let (alone, _, complaint) = git_with(&project, &pack, &["index-pack", "--stdin"]).await?;
        assert!(
            !alone && complaint.contains("unresolved delta"),
            "{complaint}"
        );
        let (fixed, _, _) = git_with(
            lab.rig.repo.root().as_path(),
            &pack,
            &["index-pack", "--stdin", "--fix-thin"],
        )
        .await?;
        assert!(fixed, "the control: the host completes the thin pack");

        let honest = lab.honest().await?;
        let head = &honest[..header_len(&honest)];
        let thin = [head.to_vec(), pack].concat();
        let before = snapshot(&lab.rig).await?;
        lab.tap.replace_next_download(thin);

        let refused = lab.export().await;

        let Err(ExportError::HostGit { output, .. }) = refused else {
            return Err(format!("expected a host git refusal, saw {refused:?}").into());
        };
        assert!(output.as_str().contains("unresolved delta"), "{output:?}");
        let after = snapshot(&lab.rig).await?;
        assert!(exports(&after).is_empty());
        assert_eq!(after.rest, before.rest);
        assert_eq!(staging_left(&lab.rig)?, Vec::<String>::new());
        lab.export().await.map_err(|error| format!("{error:?}"))?;
        Ok(())
    })
    .await
}
