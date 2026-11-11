// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::RepoFixture;

use super::oracle::arrived;
use super::support::{Rig, TestResult, global_config, guard, ready};

async fn equal_trees(repo: RepoFixture, global: Option<&str>, dirty: &[&str]) -> TestResult {
    let rig = Rig::new(repo).await?;
    if let Some(text) = global {
        let ignore = rig.world.home().as_path().join("ignore");
        std::fs::write(&ignore, text)?;
        global_config(
            &rig,
            &format!("[core]\n\texcludesFile = {}\n", ignore.display()),
        )?;
    }
    ready(rig.launch(1).await?)?;
    let handle = rig.handle(1).await?;
    arrived(rig.repo.root().as_path(), &handle.root(), dirty)
}

#[tokio::test]
async fn a_tracked_file_that_an_exclude_rule_matches_is_in_both_trees() -> TestResult {
    guard(async {
        let junk = RepoFixture::new().await?;
        junk.file("a.txt", b"alpha\n").await?;
        junk.file(".DS_Store", b"tracked finder junk\n").await?;
        junk.commit().await?;
        equal_trees(junk, Some(".DS_Store\n"), &[]).await?;

        let ignored = RepoFixture::new().await?;
        ignored.file("secret.txt", b"tracked\n").await?;
        ignored.commit().await?;
        ignored.ignore("secret.txt\n").await?;
        ignored.commit().await?;
        equal_trees(ignored, None, &[]).await
    })
    .await
}

#[tokio::test]
async fn glob_characters_and_odd_names_are_listed_literally_and_ignored_files_stay_out()
-> TestResult {
    guard(async {
        let globs = RepoFixture::new().await?;
        globs.ignore("build.dat\n").await?;
        globs.file("*.dat", b"star\n").await?;
        globs.file("[ab].txt", b"class\n").await?;
        globs.commit().await?;
        globs.file("build.dat", b"ignored\n").await?;
        equal_trees(globs, None, &[]).await?;

        let names = RepoFixture::new().await?;
        names.file("sp ace.txt", b"space\n").await?;
        names.file("new\nline.txt", b"newline\n").await?;
        names.commit().await?;
        equal_trees(names, None, &[]).await?;

        let deleted = RepoFixture::new().await?;
        deleted.file("keep.txt", b"keep\n").await?;
        deleted.file("gone.txt", b"gone\n").await?;
        deleted.commit().await?;
        deleted.remove("gone.txt").await?;
        equal_trees(deleted, None, &[" D gone.txt"]).await
    })
    .await
}
