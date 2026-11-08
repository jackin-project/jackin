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
        assert_eq!(model.as_deref(), Some("gpt-5.1-codex"));
        let moved = origin.advance("gpt-5.1-codex").await?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("gpt-5.1-codex")));
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
            (origin.commits[1].clone(), Some("gpt-5.1-codex"))
        );
        let moved = origin.advance("gpt-5.1-codex").await?;
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
async fn a_fully_qualified_branch_revision_tracks_the_remote_branch() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, Some("refs/heads/main"))
            .await?
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            (commit, model.as_deref()),
            (origin.commits[0].clone(), Some("gpt-5.1-codex"))
        );

        let moved = origin.advance("revision-2").await?;
        let (commit, model) = origin
            .load(&cache, Some("refs/heads/main"))
            .await?
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("gpt-5.1-codex")));

        let missing = origin.load(&cache, Some("refs/heads/missing")).await?;
        assert!(matches!(missing, Err(RoleError::Git { .. })), "{missing:?}");
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
                (origin.commits[0].clone(), Some("gpt-5.1-codex")),
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
            (origin.commits[0].clone(), Some("gpt-5.1-codex"))
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
            (origin.commits[1].clone(), Some("gpt-5.1-codex"))
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
        assert_eq!(model.as_deref(), Some("gpt-5.1-codex"));
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
        assert_eq!(model.as_deref(), Some("gpt-5.1-codex"));
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
async fn a_pinned_role_loads_from_its_cache_while_the_origin_head_dangles() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        origin
            .load(&cache, Some("v1"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        git(
            origin.role.root().as_path(),
            &["symbolic-ref", "HEAD", "refs/heads/gone"],
        )
        .await?;
        let (commit, _) = origin
            .load(&cache, Some("v1"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, origin.commits[0]);
        let followed = origin.load(&cache, None).await?;
        assert!(
            matches!(followed, Err(RoleError::Git { .. })),
            "{followed:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_locks_live_apart_from_the_role_checkouts_and_the_cache_directory_is_private()
-> TestResult {
    guard(async {
        use std::os::unix::fs::PermissionsExt;

        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let roles = cache.as_path().join("roles");
        tokio::fs::create_dir_all(&roles).await?;
        tokio::fs::set_permissions(&roles, std::fs::Permissions::from_mode(0o755)).await?;
        origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        let mut entries = Vec::new();
        let mut listing = tokio::fs::read_dir(&roles).await?;
        while let Some(entry) = listing.next_entry().await? {
            entries.push(entry.file_name().to_string_lossy().into_owned());
        }
        entries.sort();
        assert_eq!(entries, [".locks", "sentinel"]);
        let mode = tokio::fs::metadata(&roles).await?.permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        Ok(())
    })
    .await
}
