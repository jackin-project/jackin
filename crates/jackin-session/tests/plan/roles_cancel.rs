// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{FileSystem, HostPath, ProcessRunner, RoleName, StdFileSystem, WorkspaceName};
use jackin_session::{RoleCatalog, RoleError};
use jackin_testkit::{CancelsOn, FakeClock, RoleFixture, TestRunner, WorkingFs};
use tokio_util::sync::CancellationToken;

use super::roles_origin::{Origin, cache, git};
use super::support::{TestResult, guard};

fn catalog_over(runner: Arc<dyn ProcessRunner>, cache: &HostPath) -> RoleCatalog {
    let clock = Arc::new(FakeClock::new());
    let fs: Arc<dyn FileSystem> =
        Arc::new(WorkingFs::new(Arc::new(StdFileSystem::default()), &clock));
    RoleCatalog::new(fs, runner, clock, cache.clone())
}

async fn load_cancelled_at(
    origin: &Origin,
    cache: &HostPath,
    word: &'static str,
) -> Result<Result<(), RoleError>, Box<dyn std::error::Error>> {
    let token = CancellationToken::new();
    let real = Arc::new(TestRunner::with_home(&origin.home));
    let runner: Arc<dyn ProcessRunner> = Arc::new(CancelsOn::new(&real, word, &token));
    let catalog = catalog_over(runner, cache);
    let config = origin.config(None)?;
    let workspace = config
        .workspace(&WorkspaceName::parse("w")?)
        .ok_or("no workspace")?;
    let name = RoleName::parse("sentinel")?;
    Ok(catalog
        .load(&config, Some(&name), workspace, &token)
        .await
        .map(|_| ()))
}

#[tokio::test]
async fn a_cancel_at_clone_fetch_or_checkout_ends_the_load_and_moves_nothing() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;

        let (fresh, _keep) = cache()?;
        let found = load_cancelled_at(&origin, &fresh, "clone").await?;
        assert!(matches!(found, Err(RoleError::Cancelled)), "{found:?}");
        assert!(!fresh.as_path().join("roles/sentinel").exists());

        let (cached, _keep) = cache()?;
        origin
            .load(&cached, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        let tree = cached.as_path().join("roles/sentinel");
        let before = git(&tree, &["rev-parse", "HEAD"]).await?;
        origin.advance("gpt-5.1-codex").await?;
        for word in ["fetch", "checkout"] {
            let found = load_cancelled_at(&origin, &cached, word).await?;
            assert!(
                matches!(found, Err(RoleError::Cancelled)),
                "{word}: {found:?}"
            );
            assert_eq!(git(&tree, &["rev-parse", "HEAD"]).await?, before, "{word}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_at_an_inspection_call_ends_load_path() -> TestResult {
    guard(async {
        let (cache, _keep) = cache()?;
        for word in ["rev-parse", "ls-files"] {
            let role = RoleFixture::new("sentinel")?;
            role.commit().await?;
            let token = CancellationToken::new();
            let real = Arc::new(TestRunner::new());
            let runner: Arc<dyn ProcessRunner> = Arc::new(CancelsOn::new(&real, word, &token));
            let found = catalog_over(runner, &cache)
                .load_path(role.root(), &token)
                .await;
            assert!(
                matches!(found, Err(RoleError::Cancelled)),
                "{word}: {found:?}"
            );
        }
        Ok(())
    })
    .await
}
