// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::RepoFixture;

use super::oracle::{arrived, git};
use super::support::{Rig, TestResult, guard, ready};

#[tokio::test]
async fn ambient_git_variables_of_the_host_process_never_reach_the_project_git_calls() -> TestResult
{
    guard(async {
        let decoy = RepoFixture::new().await?;
        decoy.file("decoy.txt", b"decoy\n").await?;
        decoy.commit().await?;
        let decoy_head = decoy.git(&["rev-parse", "HEAD"]).await?;
        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.commit().await?;
        repo.file("u.txt", b"untracked\n").await?;
        let rig = Rig::new(repo).await?;
        let hook = rig.world.home().as_path().join("fsmonitor-ran");
        let runner = &rig.world.runner;
        let decoy_root = decoy.root().as_path();
        runner.set_ambient("GIT_DIR", &decoy_root.join(".git").display().to_string());
        runner.set_ambient("GIT_WORK_TREE", &decoy_root.display().to_string());
        runner.set_ambient(
            "GIT_INDEX_FILE",
            &decoy_root.join(".git/index").display().to_string(),
        );
        runner.set_ambient("GIT_CONFIG_COUNT", "1");
        runner.set_ambient("GIT_CONFIG_KEY_0", "core.fsmonitor");
        runner.set_ambient(
            "GIT_CONFIG_VALUE_0",
            &format!("touch {} && true", hook.display()),
        );
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        arrived(rig.repo.root().as_path(), &handle.root(), &["?? u.txt"])?;
        assert!(!hook.exists());
        assert_eq!(
            git(decoy.root().as_path(), &["rev-parse", "HEAD"])?,
            decoy_head
        );
        assert_eq!(git(decoy.root().as_path(), &["status", "--porcelain"])?, "");
        Ok(())
    })
    .await
}
