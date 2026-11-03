// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::BranchName;
use jackin_session::{GithubOrigin, GithubRepo};
use jackin_testkit::{Rig, TestResult, guard};
use tokio_util::sync::CancellationToken;

async fn origin(rig: &Rig) -> Result<Option<GithubOrigin>, Box<dyn std::error::Error>> {
    Ok(rig
        .world
        .session
        .project_origin(&rig.project, &CancellationToken::new())
        .await?)
}

fn repo(owner: &str, name: &str) -> Option<(String, String)> {
    Some((owner.to_owned(), name.to_owned()))
}

fn pair(found: Option<GithubOrigin>) -> Option<(String, String)> {
    found.map(|found| (found.repo.owner().to_owned(), found.repo.name().to_owned()))
}

#[tokio::test]
async fn only_the_origin_remote_on_the_exact_github_host_is_a_github_source() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        assert_eq!(origin(&rig).await?, None, "no origin at all");

        let cases = [
            ("git@github.com:acme/widgets.git", repo("acme", "widgets")),
            ("https://github.com/acme/widgets", repo("acme", "widgets")),
            ("ssh://git@github.com/acme/widgets", repo("acme", "widgets")),
            (
                "https://github.com/acme/wid.get_s.git",
                repo("acme", "wid.get_s"),
            ),
            ("https://github.com.evil.example/acme/widgets", None),
            ("git@github.com.evil.example:acme/widgets.git", None),
            ("https://notgithub.com/acme/widgets", None),
            ("git@notgithub.com:acme/widgets.git", None),
            ("https://github.com/acme", None),
            ("https://github.com/acme/../widgets", None),
            ("https://example.org/acme/widgets", None),
        ];
        for (index, (url, expected)) in cases.into_iter().enumerate() {
            if index > 0 {
                rig.repo.git(&["remote", "remove", "origin"]).await?;
            }
            rig.repo.git(&["remote", "add", "origin", url]).await?;
            assert_eq!(pair(origin(&rig).await?), expected, "{url}");
        }

        rig.repo.git(&["remote", "remove", "origin"]).await?;
        rig.repo
            .git(&[
                "remote",
                "add",
                "origin",
                "https://example.org/acme/widgets",
            ])
            .await?;
        rig.repo
            .git(&[
                "remote",
                "add",
                "upstream",
                "https://github.com/acme/widgets",
            ])
            .await?;
        assert_eq!(origin(&rig).await?, None, "upstream is not origin");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_default_branch_comes_from_the_origin_head_and_is_absent_without_it() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo
            .git(&["remote", "add", "origin", "git@github.com:acme/widgets.git"])
            .await?;
        let bare = origin(&rig).await?.ok_or("no github origin")?;
        assert_eq!(bare.default_branch, None);
        assert_eq!(bare.repo.to_string(), "https://github.com/acme/widgets");

        rig.repo
            .git(&[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ])
            .await?;
        let with_head = origin(&rig).await?.ok_or("no github origin")?;
        assert_eq!(with_head.default_branch, Some(BranchName::parse("trunk")?));
        assert_eq!(
            GithubRepo::parse("git@github.com:acme/widgets"),
            Some(with_head.repo)
        );
        Ok(())
    })
    .await
}
