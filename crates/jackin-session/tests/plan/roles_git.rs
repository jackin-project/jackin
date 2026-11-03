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
        assert_eq!(model.as_deref(), Some("model-1"));
        let moved = origin.advance("model-2").await?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("model-2")));
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
            (origin.commits[1].clone(), Some("model-dev"))
        );
        let moved = origin.advance("model-2").await?;
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
                (origin.commits[0].clone(), Some("model-1")),
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
            (origin.commits[0].clone(), Some("model-1"))
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
            (origin.commits[1].clone(), Some("model-dev"))
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
        assert_eq!(model.as_deref(), Some("model-dev"));
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
        assert_eq!(model.as_deref(), Some("model-1"));
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
