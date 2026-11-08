// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ConfigEdit, Workspace, WorkspaceName};
use jackin_testkit::{RepoFixture, Rig, TestResult, guard};

use super::support::Lab;

type Out = Result<(), Box<dyn std::error::Error>>;

async fn with_remote(rig: &Rig, remote: &str, url: &str) -> Out {
    rig.repo.git(&["remote", "add", remote, url]).await?;
    Ok(())
}

async fn extra(
    lab: &Lab,
    name: &str,
    remote: Option<(&str, &str)>,
) -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"a").await?;
    repo.commit().await?;
    if let Some((remote, url)) = remote {
        repo.git(&["remote", "add", remote, url]).await?;
    }
    lab.edit(vec![ConfigEdit::AddWorkspace {
        name: WorkspaceName::parse(name)?,
        workspace: Workspace::new(repo.root().clone(), false),
    }])
    .await?;
    Ok(repo)
}

#[tokio::test]
async fn origin_reaches_the_instance_and_o_shows_it() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        with_remote(&rig, "origin", "git@github.com:acme/widgets.git").await?;
        rig.repo
            .git(&["update-ref", "refs/remotes/origin/main", "HEAD"])
            .await?;
        rig.repo
            .git(&[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ])
            .await?;
        rig.launch(1).await?;
        let mut lab = Lab::with(rig).await?;
        let world = lab.world();
        assert_eq!(world.github.len(), 1);
        assert_eq!(world.github[0].full_name, "acme/widgets");
        assert_eq!(world.github[0].url, "https://github.com/acme/widgets");
        assert_eq!(world.github[0].default_branch, "main");
        assert_eq!(world.github[0].branches, ["main"]);
        let first = world.instances[0]
            .as_available()
            .ok_or("instance metadata unavailable")?;
        let repo = first.github.clone().ok_or("no repo on the instance")?;
        assert_eq!(repo.url, "https://github.com/acme/widgets");
        assert_eq!(first.default_branch, "main");
        lab.console.keys("down o")?;
        let status = lab.console.status_line();
        assert!(
            status.contains("https://github.com/acme/widgets"),
            "{status}"
        );
        assert!(
            !status.contains("Opened") && !status.contains("Revealed"),
            "{status}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn only_a_github_origin_counts() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        with_remote(
            &rig,
            "origin",
            "https://github.com.evil.example/acme/widgets",
        )
        .await?;
        rig.launch(1).await?;
        let lab = Lab::with(rig).await?;
        let _kept = [
            extra(
                &lab,
                "lookalike",
                Some(("origin", "https://notgithub.com/acme/widgets")),
            )
            .await?,
            extra(
                &lab,
                "upstream-only",
                Some(("upstream", "git@github.com:acme/widgets.git")),
            )
            .await?,
            extra(&lab, "no-remote", None).await?,
        ];
        let mut lab = lab;
        lab.reopen().await?;
        let world = lab.world();
        assert!(world.github.is_empty(), "{:?}", world.github);
        assert!(
            world
                .instances
                .iter()
                .filter_map(jackin_console::domain::instance::InstanceEntry::as_available)
                .all(|instance| instance.github.is_none())
        );
        lab.console.keys("down down down o")?;
        let status = lab.console.status_line();
        assert_eq!(status, "No GitHub source for platform-backend");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn no_origin_head_shows_no_invented_branch() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        with_remote(&rig, "origin", "https://github.com/acme/widgets").await?;
        let mut lab = Lab::with(rig).await?;
        let world = lab.world();
        assert_eq!(world.github.len(), 1);
        assert_eq!(world.github[0].default_branch, "");
        assert!(world.github[0].branches.is_empty());
        Ok(())
    })
    .await
}
