// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::RoleError;

use super::roles_origin::{Origin, cache, git};
use super::support::{TestResult, guard};

#[tokio::test]
async fn a_git_role_without_a_revision_follows_the_default_branch_as_it_moves() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            Some(commit),
            git(origin.role.root().as_path(), &["rev-parse", "main"])
                .await
                .ok()
        );
        assert_eq!(model.as_deref(), Some("glm-5.3"));
        let moved = origin.advance("glm-5.3").await?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("glm-5.3")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_follows_a_non_default_branch_and_its_remote_updates() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, Some("dev"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (commit, model.as_deref()),
            (origin.commits[1].clone(), Some("glm-5.3"))
        );
        let moved = origin.advance("glm-5.3").await?;
        let (commit, _) = origin
            .load(&cache, Some("main"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, moved);
        let (commit, _) = origin
            .load(&cache, Some("main"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, moved);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_pinned_by_a_fully_qualified_branch_ref_follows_the_origin_branch() -> TestResult
{
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, Some("refs/heads/main"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (commit, model.as_deref()),
            (origin.commits[0].clone(), Some("glm-5.3"))
        );
        let moved = origin.advance("glm-5.3").await?;
        let (commit, model) = origin
            .load(&cache, Some("refs/heads/main"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("glm-5.3")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_loads_a_tag_and_a_commit_and_refuses_an_unknown_revision() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        for rev in ["v1", origin.commits[0].as_str()] {
            let (commit, model) = origin
                .load(&cache, Some(rev))
                .await?
                .map_err(|e| format!("{e:?}"))?;
            assert_eq!(
                (commit, model.as_deref()),
                (origin.commits[0].clone(), Some("glm-5.3")),
                "{rev}"
            );
        }
        let missing = origin.load(&cache, Some("later")).await?;
        assert!(matches!(missing, Err(RoleError::Git { .. })), "{missing:?}");
        git(origin.role.root().as_path(), &["branch", "later", "dev"]).await?;
        let (commit, _) = origin
            .load(&cache, Some("later"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, origin.commits[1]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_loads_the_new_commit_when_origin_moves_a_tag() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, Some("v1"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (commit, model.as_deref()),
            (origin.commits[0].clone(), Some("glm-5.3"))
        );
        git(
            origin.role.root().as_path(),
            &["tag", "-f", "v1", &origin.commits[1]],
        )
        .await?;
        let (commit, model) = origin
            .load(&cache, Some("v1"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (commit, model.as_deref()),
            (origin.commits[1].clone(), Some("glm-5.3"))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_whose_branch_was_deleted_at_the_origin_no_longer_loads() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (_, model) = origin
            .load(&cache, Some("dev"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(model.as_deref(), Some("glm-5.3"));
        git(origin.role.root().as_path(), &["branch", "-D", "dev"]).await?;
        let gone = origin.load(&cache, Some("dev")).await?;
        assert!(
            matches!(gone, Err(RoleError::Git { .. })),
            "the deleted branch still loads: {gone:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_whose_tag_was_deleted_at_the_origin_no_longer_loads() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (_, model) = origin
            .load(&cache, Some("v1"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(model.as_deref(), Some("glm-5.3"));
        git(origin.role.root().as_path(), &["tag", "-d", "v1"]).await?;
        let gone = origin.load(&cache, Some("v1")).await?;
        assert!(
            matches!(gone, Err(RoleError::Git { .. })),
            "the deleted tag still loads: {gone:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn roles_named_a_and_a_dot_lock_load_in_either_order_without_sharing_a_lock_path()
-> TestResult {
    guard(async {
        for order in [["a", "a.lock"], ["a.lock", "a"]] {
            let (cache, _keep) = cache()?;
            let origins = [
                Origin::named("a", "https://example.invalid/a.git").await?,
                Origin::named("a.lock", "https://example.invalid/a-lock.git").await?,
            ];
            for name in order {
                let origin = origins
                    .iter()
                    .find(|origin| origin.name == name)
                    .ok_or("origin")?;
                let (commit, _) = origin
                    .load(&cache, None)
                    .await?
                    .map_err(|e| format!("{name}: {e:?}"))?;
                assert_eq!(
                    Some(commit),
                    git(origin.role.root().as_path(), &["rev-parse", "main"])
                        .await
                        .ok(),
                    "{name}"
                );
            }
            for origin in &origins {
                origin
                    .load(&cache, None)
                    .await?
                    .map_err(|e| format!("{}: {e:?}", origin.name))?;
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_pinned_role_updates_while_the_origin_head_points_nowhere_and_an_unpinned_one_follows_a_new_default()
-> TestResult {
    guard(async {
        for (rev, wanted) in [("v1", 0_usize), ("dev", 1)] {
            let origin = Origin::new().await?;
            let (cache, _keep) = cache()?;
            let first = origin
                .load(&cache, Some(rev))
                .await?
                .map_err(|e| format!("{e:?}"))?;
            assert_eq!(first.0, origin.commits[wanted]);
            git(
                origin.role.root().as_path(),
                &["symbolic-ref", "HEAD", "refs/heads/gone"],
            )
            .await?;
            let again = origin
                .load(&cache, Some(rev))
                .await?
                .map_err(|e| format!("{rev}: {e:?}"))?;
            assert_eq!(again.0, origin.commits[wanted], "{rev}");
        }
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        git(
            origin.role.root().as_path(),
            &["symbolic-ref", "HEAD", "refs/heads/dev"],
        )
        .await?;
        let (commit, _) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, origin.commits[1]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_revision_naming_the_remote_head_follows_a_new_default_branch_like_an_unpinned_role()
-> TestResult {
    guard(async {
        for rev in ["HEAD", "origin/HEAD", "remotes/origin/HEAD", "refs/remotes/origin/HEAD"] {
            let origin = Origin::new().await?;
            let (cache, _keep) = cache()?;
            origin
                .load(&cache, Some(rev))
                .await?
                .map_err(|e| format!("{rev}: {e:?}"))?;
            git(
                origin.role.root().as_path(),
                &["symbolic-ref", "HEAD", "refs/heads/dev"],
            )
            .await?;
            let (commit, _) = origin
                .load(&cache, Some(rev))
                .await?
                .map_err(|e| format!("{rev}: {e:?}"))?;
            assert_eq!(commit, origin.commits[1], "{rev}");
        }
        Ok(())
    })
    .await
}
