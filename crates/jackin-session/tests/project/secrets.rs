// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::path::Path;

use jackin_testkit::RepoFixture;

use super::oracle::{arrived, git, run};
use super::support::{Rig, TestResult, guard, ready};

async fn base() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Ok(repo)
}

fn branch_refs(dir: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    Ok(git(dir, &["for-each-ref", "--format=%(refname)", "refs/"])?
        .lines()
        .map(str::to_owned)
        .collect())
}

#[tokio::test]
async fn no_jackin_ref_reaches_the_sandbox_even_after_an_export_between_two_launches() -> TestResult
{
    guard(async {
        let repo = base().await?;
        repo.git(&["branch", "feat"]).await?;
        repo.git(&["branch", "jackin/old/main"]).await?;
        repo.git(&["checkout", "-q", "-b", "jackin/old/feat"])
            .await?;
        repo.file("a.txt", b"only on the exported branch\n").await?;
        repo.commit().await?;
        let exported = repo.git(&["rev-parse", "HEAD"]).await?;
        repo.git(&["checkout", "-q", "main"]).await?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let first = rig.handle(1).await?;
        let project = first.root().join("project");
        let expected: BTreeSet<String> = ["refs/heads/feat", "refs/heads/main"]
            .map(str::to_owned)
            .into();
        assert_eq!(branch_refs(&project)?, expected);
        assert!(
            !run(&project, &["cat-file", "-e", exported.trim()])?
                .status
                .success(),
            "the commit of a jackin branch is in the sandbox object store"
        );
        arrived(rig.repo.root().as_path(), &first.root(), &[])?;

        let source = project.display().to_string();
        rig.repo
            .git(&[
                "fetch",
                "-q",
                &source,
                "+refs/heads/*:refs/heads/jackin/box/*",
            ])
            .await?;
        assert!(branch_refs(rig.repo.root().as_path())?.contains("refs/heads/jackin/box/main"));
        ready(rig.launch(2).await?)?;
        let second = rig.handle(2).await?;
        assert_eq!(branch_refs(&second.root().join("project"))?, expected);
        arrived(rig.repo.root().as_path(), &second.root(), &[])
    })
    .await
}

#[tokio::test]
async fn a_host_head_on_a_jackin_branch_arrives_detached_at_its_commit() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&["checkout", "-q", "-b", "jackin/old/main"])
            .await?;
        repo.file("a.txt", b"exported work\n").await?;
        repo.commit().await?;
        let commit = repo.git(&["rev-parse", "HEAD"]).await?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let project = handle.root().join("project");
        assert!(
            !run(&project, &["symbolic-ref", "-q", "HEAD"])?
                .status
                .success()
        );
        assert_eq!(git(&project, &["rev-parse", "HEAD"])?, commit);
        assert_eq!(
            branch_refs(&project)?,
            ["refs/heads/main".to_owned()].into()
        );
        git(&project, &["fsck", "--full"])?;
        assert_eq!(git(&project, &["status", "--porcelain"])?, "");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unborn_head_with_only_jackin_branches_launches_unborn_without_a_bundle() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&["branch", "jackin/old/main"]).await?;
        repo.git(&["checkout", "-q", "--orphan", "fresh"]).await?;
        repo.git(&["branch", "-D", "main"]).await?;
        repo.git(&["rev-parse", "-q", "--verify", "HEAD"])
            .await
            .unwrap_err();
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let project = handle.root().join("project");
        assert!(branch_refs(&project)?.is_empty());
        arrived(rig.repo.root().as_path(), &handle.root(), &["?? a.txt"])
    })
    .await
}

const SECRETS: [&str; 5] = [
    "SECRETTOKEN-remote",
    "SECRETHEADER-http",
    "SECRETHELPER-credential",
    "SECRETSUBMODULE-config",
    "SECRETREWRITE-url",
];

fn scan(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            scan(&entry.path())?;
        } else {
            let text = String::from_utf8_lossy(&std::fs::read(entry.path())?).into_owned();
            for secret in SECRETS {
                assert!(
                    !text.contains(secret),
                    "{secret} in {}",
                    entry.path().display()
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn no_host_credential_reaches_any_sandbox_file_or_the_sandbox_repository_settings()
-> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&[
            "remote",
            "add",
            "origin",
            "https://bot:SECRETTOKEN-remote@example.invalid/x.git",
        ])
        .await?;
        repo.git(&["config", "http.extraheader", "Authorization: SECRETHEADER-http"])
            .await?;
        repo.git(&["config", "credential.helper", "store --file=SECRETHELPER-credential"])
            .await?;
        repo.git(&[
            "config",
            "url.https://bot:SECRETREWRITE-url@example.invalid/.insteadOf",
            "https://example.invalid/",
        ])
        .await?;
        repo.file(
            ".git/modules/lib/config",
            b"[remote \"origin\"]\n\turl = https://bot:SECRETSUBMODULE-config@example.invalid/lib.git\n",
        )
        .await?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        scan(&handle.root())?;
        let listed = git(
            &handle.root().join("project"),
            &["config", "--local", "--list"],
        )?;
        for key in ["remote.", "credential.", "http.", "include", "url."] {
            assert!(
                !listed.lines().any(|line| line.starts_with(key)),
                "{key}: {listed}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_host_exclude_file_travels_into_the_sandbox_repository_byte_for_byte() -> TestResult {
    guard(async {
        let exclude: &[u8] = b"local-only.log\n\xff-binary-line\n";
        let plain = base().await?;
        plain.file(".git/info/exclude", exclude).await?;
        let main = base().await?;
        main.file(".git/info/exclude", exclude).await?;
        let tree = main.linked_worktree("topic").await?;
        let bare = base().await?;
        std::fs::remove_file(bare.root().as_path().join(".git/info/exclude"))?;

        for (repo, carried) in [(plain, true), (tree, true), (bare, false)] {
            let rig = Rig::new(repo).await?;
            ready(rig.launch(1).await?)?;
            let handle = rig.handle(1).await?;
            let project = handle.root().join("project");
            std::fs::write(project.join("local-only.log"), b"log\n")?;
            let status = git(
                &project,
                &["status", "--porcelain", "--untracked-files=all"],
            )?;
            let written = std::fs::read(project.join(".git/info/exclude"))?;
            if carried {
                assert_eq!(status, "");
                assert_eq!(written, exclude);
            } else {
                assert_eq!(status, "?? local-only.log\n");
            }
        }
        Ok(())
    })
    .await
}
