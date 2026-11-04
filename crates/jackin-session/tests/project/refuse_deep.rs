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
    repo.commit().await?;
    Ok(repo)
}

async fn refused(repo: RepoFixture) -> Result<UnsupportedProject, Box<dyn std::error::Error>> {
    let rig = Rig::new(repo).await?;
    let refusal = unsupported(rig.launch(1).await?)?;
    assert_nothing_created(&rig)?;
    Ok(refusal)
}

fn names(paths: &[RepoPath]) -> Vec<&str> {
    paths.iter().map(RepoPath::as_str).collect()
}

#[tokio::test]
async fn a_gitlink_with_any_content_is_refused_however_deep_the_nested_repository_sits()
-> TestResult {
    guard(async {
        for (file, bytes) in [
            ("lib/inner/.git/config", b"[core]\n".as_slice()),
            ("lib/loose.txt", b"loose\n".as_slice()),
        ] {
            let outer = base().await?.superproject().await?;
            outer.git(&["submodule", "deinit", "--all", "-f"]).await?;
            outer.file(file, bytes).await?;
            let UnsupportedProject::PopulatedSubmodules(paths) = refused(outer).await? else {
                return Err(format!("{file}: not a submodule refusal").into());
            };
            assert_eq!(names(&paths), ["lib"], "{file}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_untracked_directory_that_holds_only_a_repository_is_refused() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&["init", "-q", "vendor/dep"]).await?;
        let UnsupportedProject::NestedRepositories(paths) = refused(repo).await? else {
            return Err("not a nested repository refusal".into());
        };
        assert_eq!(names(&paths), ["vendor/dep"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_untracked_repository_inside_a_directory_the_outer_repository_tracks_is_refused()
-> TestResult {
    guard(async {
        for (nested, name) in [("lib/dep", "lib/dep"), ("lib/dep/deeper", "lib/dep/deeper")] {
            let repo = base().await?;
            repo.file("lib/dep/tracked.txt", b"tracked\n").await?;
            repo.commit().await?;
            repo.git(&["init", "-q", nested]).await?;
            repo.file(&format!("{nested}/secret.txt"), b"nested work\n")
                .await?;
            let UnsupportedProject::NestedRepositories(paths) = refused(repo).await? else {
                return Err(format!("{nested}: not a nested repository refusal").into());
            };
            assert_eq!(names(&paths), [name], "{nested}");
        }

        let plain = base().await?;
        plain.file("lib/dep/tracked.txt", b"tracked\n").await?;
        plain.commit().await?;
        plain.file("lib/dep/new.txt", b"new\n").await?;
        let rig = Rig::new(plain).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        arrived(
            rig.repo.root().as_path(),
            &handle.root(),
            &["?? lib/dep/new.txt"],
        )
    })
    .await
}

#[tokio::test]
async fn a_bare_repository_stored_as_project_files_is_project_content() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.file("testdata/fixture.git/HEAD", b"ref: refs/heads/main\n")
            .await?;
        repo.file("testdata/fixture.git/config", b"[core]\n\tbare = true\n")
            .await?;
        repo.file("testdata/fixture.git/objects/info/packs", b"\n")
            .await?;
        repo.file("testdata/fixture.git/refs/heads/main", b"0000\n")
            .await?;
        repo.commit().await?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        arrived(rig.repo.root().as_path(), &handle.root(), &[])?;
        assert!(
            handle
                .root()
                .join("project/testdata/fixture.git/HEAD")
                .is_file()
        );
        Ok(())
    })
    .await
}
