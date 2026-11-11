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

const GITHUB_URL: &str = "https://github.com/acme/widgets";
const EVIL_URL: &str = "https://evil.example/x.git";

async fn configured(
    urls: &[&str],
    settings: &[(&str, &str)],
) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    let rig = Rig::new().await?;
    for url in urls {
        rig.repo
            .git(&["config", "--add", "remote.origin.url", url])
            .await?;
    }
    for (key, value) in settings {
        rig.repo.git(&["config", key, value]).await?;
    }
    Ok(pair(origin(&rig).await?))
}

#[tokio::test]
async fn the_origin_is_the_url_git_fetches_from_so_a_second_url_or_a_rewrite_cannot_pass_as_github()
-> TestResult {
    guard(async {
        let rewrite_to_github = ("url.https://github.com/.insteadOf", "gh:");
        let rewrite_away = ("url.https://evil.example/.insteadOf", "https://github.com/");
        let found = [
            (
                "one plain github url",
                configured(&[GITHUB_URL], &[]).await?,
            ),
            (
                "a foreign url first, github second",
                configured(&[EVIL_URL, GITHUB_URL], &[]).await?,
            ),
            (
                "github first, a foreign url second",
                configured(&[GITHUB_URL, EVIL_URL], &[]).await?,
            ),
            (
                "a rewrite that leads to github",
                configured(&["gh:acme/widgets"], &[rewrite_to_github]).await?,
            ),
            (
                "a rewrite that leads away from github",
                configured(&[GITHUB_URL], &[rewrite_away]).await?,
            ),
        ];
        assert_eq!(
            found,
            [
                ("one plain github url", repo("acme", "widgets")),
                ("a foreign url first, github second", None),
                ("github first, a foreign url second", None),
                ("a rewrite that leads to github", repo("acme", "widgets")),
                ("a rewrite that leads away from github", None),
            ]
        );
        Ok(())
    })
    .await
}
