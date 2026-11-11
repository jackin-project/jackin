// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::symlink;
use std::path::Path;

use jackin_core::RepoPath;
use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;

use super::oracle::arrived;
use super::support::{Rig, TestResult, assert_nothing_created, guard, ready, unsupported};

const OUTSIDE: &str = "HOST-SECRET-TOKEN";

enum Layout {
    Listed,
    Ignored,
    Middle,
}

fn outside() -> Result<tempfile::TempDir, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("token.txt"), OUTSIDE)?;
    Ok(dir)
}

async fn linked(layout: &Layout, outside: &Path) -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    let root = repo.root().as_path().to_path_buf();
    match layout {
        Layout::Listed | Layout::Ignored => {
            repo.file("creds/token.txt", b"tracked\n").await?;
            repo.commit().await?;
            std::fs::remove_dir_all(root.join("creds"))?;
            symlink(outside, root.join("creds"))?;
            if matches!(layout, Layout::Ignored) {
                repo.ignore("creds\n").await?;
            }
        }
        Layout::Middle => {
            repo.file("real/c.txt", b"inside\n").await?;
            repo.file("a/b/c.txt", b"tracked\n").await?;
            repo.commit().await?;
            std::fs::remove_dir_all(root.join("a/b"))?;
            symlink(root.join("real"), root.join("a/b"))?;
        }
    }
    Ok(repo)
}

fn scan(dir: &Path) -> TestResult {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            scan(&entry.path())?;
        } else if entry.file_type()?.is_file()
            && String::from_utf8_lossy(&std::fs::read(entry.path())?).contains(OUTSIDE)
        {
            return Err(format!("{OUTSIDE} reached {}", entry.path().display()).into());
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_listed_path_below_a_symlinked_directory_is_refused_before_anything_is_created()
-> TestResult {
    guard(async {
        let cases = [
            ("listed link", Layout::Listed, "creds/token.txt"),
            ("ignored link", Layout::Ignored, "creds/token.txt"),
            ("middle link", Layout::Middle, "a/b/c.txt"),
        ];
        for (label, layout, expected) in cases {
            let outside = outside()?;
            let rig = Rig::new(linked(&layout, outside.path()).await?).await?;
            let outcome = rig.launch(1).await?;
            if let Ok(handle) = rig.handle(1).await {
                scan(&handle.root()).map_err(|error| format!("{label}: {error}"))?;
            }
            let refusal = unsupported(outcome).map_err(|error| format!("{label}: {error}"))?;
            let UnsupportedProject::SymlinkedPath(paths) = &refusal else {
                return Err(format!("{label}: not a symlinked path refusal: {refusal:?}").into());
            };
            assert_eq!(
                paths.iter().map(RepoPath::as_str).collect::<Vec<_>>(),
                [expected],
                "{label}"
            );
            assert!(!refusal.to_string().contains(OUTSIDE), "{label}");
            assert_nothing_created(&rig).map_err(|error| format!("{label}: {error}"))?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn symlinks_with_nothing_listed_beneath_them_still_launch_and_arrive_as_links() -> TestResult
{
    guard(async {
        let outside = outside()?;
        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.file("real/x.txt", b"x\n").await?;
        repo.commit().await?;
        let root = repo.root().as_path().to_path_buf();
        symlink("a.txt", root.join("link.txt"))?;
        symlink("real", root.join("dirlink"))?;
        symlink(outside.path(), root.join("ext"))?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let project = handle.root().join("project");
        for name in ["link.txt", "dirlink", "ext"] {
            assert!(
                std::fs::symlink_metadata(project.join(name))?.is_symlink(),
                "{name} did not arrive as a symlink"
            );
        }
        arrived(
            rig.repo.root().as_path(),
            &handle.root(),
            &["?? dirlink", "?? ext", "?? link.txt"],
        )
    })
    .await
}
