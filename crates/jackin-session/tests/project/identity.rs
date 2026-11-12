// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_testkit::RepoFixture;

use super::oracle::{git, run};
use super::support::{Rig, TestResult, global_config, guard, ready};

async fn base() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Ok(repo)
}

fn commit_as_configured(project: &Path) -> Result<String, Box<dyn std::error::Error>> {
    git(
        project,
        &[
            "-c",
            "user.useConfigOnly=true",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "probe",
        ],
    )?;
    git(project, &["log", "-1", "--format=%an|%ae|%cn|%ce"])
}

async fn launched(rig: &Rig) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    ready(rig.launch(1).await?)?;
    Ok(rig.handle(1).await?.root())
}

#[tokio::test]
async fn the_effective_host_identity_reaches_the_sandbox_repository() -> TestResult {
    guard(async {
        let local = base().await?;
        local.git(&["config", "user.name", "Local Name"]).await?;
        local
            .git(&["config", "user.email", "local@example.invalid"])
            .await?;
        let rig = Rig::new(local).await?;
        let root = launched(&rig).await?;
        assert_eq!(
            commit_as_configured(&root.join("project"))?,
            "Local Name|local@example.invalid|Local Name|local@example.invalid\n"
        );

        let global = Rig::new(base().await?).await?;
        global_config(
            &global,
            "[user]\n\tname = Global Name\n\temail = global@example.invalid\n",
        )?;
        let root = launched(&global).await?;
        assert_eq!(
            commit_as_configured(&root.join("project"))?,
            "Global Name|global@example.invalid|Global Name|global@example.invalid\n"
        );

        let both = base().await?;
        both.git(&["config", "user.name", "Local Name"]).await?;
        let rig = Rig::new(both).await?;
        global_config(
            &rig,
            "[user]\n\tname = Global Name\n\temail = global@example.invalid\n",
        )?;
        let root = launched(&rig).await?;
        assert_eq!(
            commit_as_configured(&root.join("project"))?,
            "Local Name|global@example.invalid|Local Name|global@example.invalid\n"
        );

        let included = Rig::new(base().await?).await?;
        let identity = included.world.home().as_path().join("work.gitconfig");
        std::fs::write(
            &identity,
            "[user]\n\tname = Work Name\n\temail = work@example.invalid\n",
        )?;
        global_config(
            &included,
            &format!(
                "[includeIf \"gitdir:{}/\"]\n\tpath = {}\n",
                included.repo.root().as_path().display(),
                identity.display()
            ),
        )?;
        let root = launched(&included).await?;
        assert_eq!(
            commit_as_configured(&root.join("project"))?,
            "Work Name|work@example.invalid|Work Name|work@example.invalid\n"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn without_an_identity_no_user_key_is_set_and_a_hostile_name_is_never_interpreted()
-> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        let root = launched(&rig).await?;
        let project = root.join("project");
        let keys = run(&project, &["config", "--local", "--get-regexp", "^user\\."])?;
        assert_eq!(keys.status.code(), Some(1));
        assert!(
            !run(
                &project,
                &[
                    "-c",
                    "user.useConfigOnly=true",
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "probe"
                ]
            )?
            .status
            .success()
        );

        let hostile = "Zo\u{eb} \"Q\" \\\\ $(touch pwned) `touch pwned2` 'x' end";
        let repo = base().await?;
        repo.git(&["config", "user.name", hostile]).await?;
        repo.git(&["config", "user.email", "h@example.invalid"])
            .await?;
        let rig = Rig::new(repo).await?;
        let root = launched(&rig).await?;
        let project = root.join("project");
        assert_eq!(
            commit_as_configured(&project)?,
            format!("{hostile}|h@example.invalid|{hostile}|h@example.invalid\n")
        );
        for name in ["pwned", "pwned2"] {
            assert!(
                !project.join(name).exists() && !root.join(name).exists(),
                "{name}"
            );
        }
        Ok(())
    })
    .await
}
