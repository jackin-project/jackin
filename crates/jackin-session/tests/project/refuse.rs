// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RepoPath;
use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;

use super::oracle::arrived;
use super::support::{Rig, TestResult, assert_nothing_created, guard, ready, unsupported};

async fn base() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.file("keep/x.txt", b"x\n").await?;
    repo.file("skip/y.txt", b"y\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn refused(repo: RepoFixture) -> Result<UnsupportedProject, Box<dyn std::error::Error>> {
    let rig = Rig::new(repo).await?;
    let refusal = unsupported(rig.launch(1).await?)?;
    assert_nothing_created(&rig)?;
    Ok(refusal)
}

async fn proceeds(repo: RepoFixture, dirty: &[&str]) -> TestResult {
    let rig = Rig::new(repo).await?;
    ready(rig.launch(1).await?)?;
    let handle = rig.handle(1).await?;
    arrived(rig.repo.root().as_path(), &handle.root(), dirty)
}

#[tokio::test]
async fn a_sparse_checkout_or_any_skip_worktree_entry_is_refused_and_assume_unchanged_is_not()
-> TestResult {
    guard(async {
        let cone = base().await?;
        cone.git(&["sparse-checkout", "init", "--cone"]).await?;
        cone.git(&["sparse-checkout", "set", "keep"]).await?;
        assert!(!cone.root().as_path().join("skip").exists());
        assert!(matches!(
            refused(cone).await?,
            UnsupportedProject::SparseCheckout
        ));

        let manual = base().await?;
        manual
            .git(&["update-index", "--skip-worktree", "a.txt"])
            .await?;
        assert!(matches!(
            refused(manual).await?,
            UnsupportedProject::SparseCheckout
        ));

        let both = base().await?;
        both.git(&["update-index", "--skip-worktree", "a.txt"])
            .await?;
        both.git(&["update-index", "--assume-unchanged", "a.txt"])
            .await?;
        assert!(
            both.git(&["ls-files", "-v", "a.txt"])
                .await?
                .starts_with("s ")
        );
        assert!(matches!(
            refused(both).await?,
            UnsupportedProject::SparseCheckout
        ));

        let assumed = base().await?;
        assumed
            .git(&["update-index", "--assume-unchanged", "a.txt"])
            .await?;
        proceeds(assumed, &[]).await
    })
    .await
}

#[tokio::test]
async fn a_populated_submodule_is_refused_by_name_and_an_empty_one_arrives() -> TestResult {
    guard(async {
        let source = base().await?;
        let outer = source.superproject().await?;
        let UnsupportedProject::PopulatedSubmodules(paths) = refused(outer.clone()).await? else {
            return Err("not a submodule refusal".into());
        };
        assert_eq!(
            paths.iter().map(RepoPath::as_str).collect::<Vec<_>>(),
            ["lib"]
        );

        outer.git(&["submodule", "deinit", "--all", "-f"]).await?;
        assert!(outer.root().as_path().join("lib").is_dir());
        proceeds(outer.clone(), &[]).await?;

        let fresh = outer.clone_of(&[]).await?;
        assert!(fresh.root().as_path().join("lib").is_dir());
        assert!(!fresh.root().as_path().join("lib/.git").exists());
        proceeds(fresh, &[]).await
    })
    .await
}

#[tokio::test]
async fn an_untracked_nested_repository_at_any_depth_is_refused_unless_it_is_ignored() -> TestResult
{
    guard(async {
        let source = base().await?;
        let url = format!("file://{}", source.root().as_path().display());
        for target in ["vendor/dep", "a/b/c"] {
            let repo = base().await?;
            repo.git(&["clone", "-q", &url, target]).await?;
            let UnsupportedProject::NestedRepositories(paths) = refused(repo).await? else {
                return Err("not a nested repository refusal".into());
            };
            assert_eq!(
                paths.iter().map(RepoPath::as_str).collect::<Vec<_>>(),
                [target]
            );
        }

        let linked = base().await?;
        let separate = tempfile::tempdir()?;
        let gitdir = separate.path().join("inner.git");
        linked
            .git(&[
                "clone",
                "-q",
                "--separate-git-dir",
                &gitdir.display().to_string(),
                &url,
                "n/inner",
            ])
            .await?;
        assert!(linked.root().as_path().join("n/inner/.git").is_file());
        assert!(matches!(
            refused(linked).await?,
            UnsupportedProject::NestedRepositories(_)
        ));

        let ignored = base().await?;
        ignored.git(&["clone", "-q", &url, "vendor/dep"]).await?;
        ignored
            .file(
                "vendor/dep/.git/config",
                concat!(
                    "[remote \"x\"]\n\turl = https://bot",
                    ":IGNORED-TOKEN@h/x.git\n"
                )
                .as_bytes(),
            )
            .await?;
        ignored.ignore("vendor/\n").await?;
        ignored.git(&["add", ".gitignore"]).await?;
        ignored.commit().await?;
        let rig = Rig::new(ignored).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        arrived(rig.repo.root().as_path(), &handle.root(), &[])?;
        for (path, bytes) in handle.files() {
            assert!(
                !String::from_utf8_lossy(&bytes).contains("IGNORED-TOKEN"),
                "{path:?}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_partial_clone_is_refused_with_the_git_output_and_a_promisor_config_alone_is_not()
-> TestResult {
    guard(async {
        let source = base().await?;
        source.file("a.txt", b"second\n").await?;
        source.commit().await?;
        source
            .git(&["config", "uploadpack.allowFilter", "true"])
            .await?;
        source
            .git(&["config", "uploadpack.allowAnySHA1InWant", "true"])
            .await?;
        let partial = source.clone_of(&["--filter=blob:none"]).await?;
        let UnsupportedProject::PartialClone { output } = refused(partial).await? else {
            return Err("not a partial clone refusal".into());
        };
        assert!(!output.as_str().trim().is_empty());

        let complete = base().await?;
        complete
            .git(&["config", "remote.origin.promisor", "true"])
            .await?;
        complete
            .git(&["config", "remote.origin.partialclonefilter", "blob:none"])
            .await?;
        proceeds(complete, &[]).await
    })
    .await
}

async fn secret_repository() -> Result<(RepoFixture, String), Box<dyn std::error::Error>> {
    let other = RepoFixture::new().await?;
    other.file("private.txt", b"TOP-SECRET-TOKEN\n").await?;
    other.commit().await?;
    let head = other.git(&["rev-parse", "HEAD"]).await?.trim().to_owned();
    Ok((other, head))
}

#[tokio::test]
async fn an_object_store_that_borrows_from_another_repository_is_refused() -> TestResult {
    guard(async {
        for name in ["alternates", "http-alternates"] {
            let (other, head) = secret_repository().await?;
            let repo = base().await?;
            let git_dir = repo.root().as_path().join(".git");
            let listed = other.root().as_path().join(".git/objects");
            let file = git_dir.join("objects/info").join(name);
            std::fs::write(&file, format!("{}\n", listed.display()))?;
            std::fs::create_dir_all(git_dir.join("refs/remotes/o"))?;
            std::fs::write(git_dir.join("refs/remotes/o/x"), format!("{head}\n"))?;
            let UnsupportedProject::Alternates(named) = refused(repo).await? else {
                return Err(format!("{name}: expected the Alternates refusal").into());
            };
            assert_eq!(named.as_path(), file, "{name}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlinked_object_store_is_refused_and_an_ordinary_clone_is_not() -> TestResult {
    guard(async {
        let repo = base().await?;
        let objects = repo.root().as_path().join(".git/objects");
        std::fs::rename(&objects, repo.root().as_path().join(".git/objects.real"))?;
        std::os::unix::fs::symlink(repo.root().as_path().join(".git/objects.real"), &objects)?;
        let UnsupportedProject::LinkedObjects(named) = refused(repo).await? else {
            return Err("expected the LinkedObjects refusal".into());
        };
        assert_eq!(named.as_path(), objects);

        proceeds(base().await?.clone_of(&[]).await?, &[]).await
    })
    .await
}
