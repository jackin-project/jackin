// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;

use super::oracle::{arrived, git, run};
use super::support::{Rig, TestResult, assert_nothing_created, guard, ready, unsupported};

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
async fn a_host_head_on_a_tag_or_a_remote_ref_arrives_detached_at_its_commit() -> TestResult {
    guard(async {
        for target in ["refs/tags/v1", "refs/remotes/origin/main"] {
            let repo = base().await?;
            repo.git(&["tag", "v1"]).await?;
            repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"])
                .await?;
            repo.git(&["symbolic-ref", "HEAD", target]).await?;
            let commit = repo.git(&["rev-parse", "HEAD"]).await?;
            let rig = Rig::new(repo).await?;
            ready(rig.launch(1).await?)?;
            let project = rig.handle(1).await?.root().join("project");
            assert!(
                !run(&project, &["symbolic-ref", "-q", "HEAD"])?
                    .status
                    .success(),
                "{target}: the sandbox HEAD is a symbolic ref"
            );
            assert_eq!(git(&project, &["rev-parse", "HEAD"])?, commit, "{target}");
            assert!(
                branch_refs(&project)?.contains("refs/heads/main"),
                "{target}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unborn_head_outside_the_plain_branches_is_refused_by_name() -> TestResult {
    guard(async {
        for target in [
            "refs/heads/jackin/old/work",
            "refs/tags/fresh",
            "refs/remotes/origin/fresh",
        ] {
            let repo = base().await?;
            repo.git(&["symbolic-ref", "HEAD", target]).await?;
            repo.git(&["rev-parse", "-q", "--verify", "HEAD"])
                .await
                .unwrap_err();
            let rig = Rig::new(repo).await?;
            match unsupported(rig.launch(1).await?)? {
                UnsupportedProject::UnbornHead { reference } => {
                    assert_eq!(reference.as_str(), target);
                }
                other => return Err(format!("{target}: expected UnbornHead, saw {other:?}").into()),
            }
            assert_nothing_created(&rig)?;
        }
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

const SECRETS: [&str; 6] = [
    "HOST-REFRESH-TOKEN-DO-NOT-LEAK",
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
            concat!("https://bot", ":SECRETTOKEN-remote@example.invalid/x.git"),
        ])
        .await?;
        repo.git(&[
            "config",
            "http.extraheader",
            "Authorization: SECRETHEADER-http",
        ])
        .await?;
        repo.git(&[
            "config",
            "credential.helper",
            "store --file=SECRETHELPER-credential",
        ])
        .await?;
        repo.git(&[
            "config",
            concat!(
                "url.https://bot",
                ":SECRETREWRITE-url@example.invalid/.insteadOf"
            ),
            "https://example.invalid/",
        ])
        .await?;
        repo.file(
            ".git/modules/lib/config",
            concat!(
                "[remote \"origin\"]\n\turl = https://bot",
                ":SECRETSUBMODULE-config@example.invalid/lib.git\n"
            )
            .as_bytes(),
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

const HOST_EXCLUDE: &[u8] =
    b"local-only.log\n{\"refresh_token\": \"HOST-REFRESH-TOKEN-DO-NOT-LEAK\"}\n";
const TEN_SECONDS: Duration = Duration::from_secs(10);

fn host_secret(dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = dir.join("token");
    std::fs::write(&path, HOST_EXCLUDE)?;
    Ok(path)
}

fn exclude_of(repo: &RepoFixture) -> PathBuf {
    repo.root().as_path().join(".git/info/exclude")
}

async fn launch_keeps_the_exclude_home(repo: RepoFixture, label: &str) -> TestResult {
    let rig = Rig::new(repo).await?;
    let launched = tokio::time::timeout(TEN_SECONDS, rig.launch(1))
        .await
        .map_err(|_| format!("{label}: read blocked, the launch outlived 10 s"))??;
    ready(launched).map_err(|error| format!("{label}: {error}"))?;
    let handle = rig.handle(1).await?;
    let project = handle.root().join("project");
    scan(&handle.root())?;
    std::fs::write(project.join("local-only.log"), b"log\n")?;
    assert_eq!(
        git(
            &project,
            &["status", "--porcelain", "--untracked-files=all"]
        )?,
        "?? local-only.log\n",
        "{label}: the sandbox applied a host exclude rule"
    );
    Ok(())
}

#[tokio::test]
async fn an_exclude_file_that_is_a_symlink_never_carries_the_file_behind_it_into_the_sandbox()
-> TestResult {
    guard(async {
        let outside = tempfile::tempdir()?;
        let secret = host_secret(outside.path())?;
        for (label, target) in [
            ("symlink to a file", secret),
            ("dangling symlink", outside.path().join("absent")),
        ] {
            let repo = base().await?;
            std::fs::remove_file(exclude_of(&repo))?;
            symlink(&target, exclude_of(&repo))?;
            launch_keeps_the_exclude_home(repo, label).await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlinked_info_directory_never_carries_its_exclude_file_into_the_sandbox() -> TestResult
{
    guard(async {
        let outside = tempfile::tempdir()?;
        let shared = outside.path().join("info");
        std::fs::create_dir(&shared)?;
        std::fs::write(shared.join("exclude"), HOST_EXCLUDE)?;
        let repo = base().await?;
        let info = repo.root().as_path().join(".git/info");
        std::fs::remove_dir_all(&info)?;
        symlink(&shared, &info)?;
        launch_keeps_the_exclude_home(repo, "symlinked info directory").await
    })
    .await
}

#[tokio::test]
async fn a_linked_worktree_with_a_symlinked_shared_exclude_never_leaks() -> TestResult {
    guard(async {
        let outside = tempfile::tempdir()?;
        let secret = host_secret(outside.path())?;
        let main = base().await?;
        std::fs::remove_file(exclude_of(&main))?;
        symlink(&secret, exclude_of(&main))?;
        let tree = main.linked_worktree("topic").await?;
        launch_keeps_the_exclude_home(tree, "linked worktree, symlinked exclude").await
    })
    .await
}
