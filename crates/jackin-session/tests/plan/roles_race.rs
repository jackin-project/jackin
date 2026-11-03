// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{FileSystem, HostPath, RoleName, StdFileSystem, WorkspaceName};
use jackin_image::RoleRevision;
use jackin_session::{LoadedRole, RoleCatalog, RoleError};
use jackin_testkit::{FakeClock, PausesOn, TestRunner};

use super::roles_origin::{Origin, cache};
use super::support::{TestResult, guard};

type Loaded = Result<(String, Option<String>), RoleError>;

fn summary(loaded: &LoadedRole) -> (String, Option<String>) {
    let (RoleRevision::Clean { commit } | RoleRevision::Dirty { commit, .. }) = loaded.revision();
    (
        commit.as_str().to_owned(),
        loaded
            .manifest()
            .codex
            .model
            .as_ref()
            .map(|model| model.as_str().to_owned()),
    )
}

struct Pair {
    first: RoleCatalog,
    second: RoleCatalog,
    pause: Arc<PausesOn>,
    clock: Arc<FakeClock>,
}

fn pair(origin: &Origin, cache: &HostPath, word: &'static str) -> Pair {
    let runner = Arc::new(TestRunner::with_home(&origin.home));
    let pause = Arc::new(PausesOn::new(&runner, word));
    let clock = Arc::new(FakeClock::new());
    let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
    Pair {
        first: RoleCatalog::new(
            Arc::clone(&fs),
            Arc::<PausesOn>::clone(&pause),
            Arc::<FakeClock>::clone(&clock),
            cache.clone(),
        ),
        second: RoleCatalog::new(fs, runner, Arc::<FakeClock>::clone(&clock), cache.clone()),
        pause,
        clock,
    }
}

async fn load(
    catalog: &RoleCatalog,
    origin: &Origin,
    rev: Option<&str>,
) -> Result<Loaded, Box<dyn std::error::Error>> {
    let config = origin.config(rev)?;
    let workspace = config
        .workspace(&WorkspaceName::parse("w")?)
        .ok_or("no workspace")?;
    let name = RoleName::parse("sentinel")?;
    Ok(catalog
        .load(&config, Some(&name), workspace)
        .await
        .map(|loaded| summary(&loaded)))
}

async fn race(
    origin: &Origin,
    word: &'static str,
    first_rev: Option<&str>,
    second_rev: Option<&str>,
) -> Result<(Loaded, Loaded), Box<dyn std::error::Error>> {
    let (cache, _keep) = cache()?;
    let pair = pair(origin, &cache, word);
    let first = load(&pair.first, origin, first_rev);
    let driver = async {
        pair.pause.reached().await;
        let second = load(&pair.second, origin, second_rev);
        tokio::pin!(second);
        let early = tokio::select! {
            biased;
            result = &mut second => Some(result),
            () = async {
                while pair.clock.sleepers() == 0 {
                    tokio::task::yield_now().await;
                }
            } => None,
        };
        pair.pause.release();
        if let Some(result) = early {
            return result;
        }
        let ticker = async {
            loop {
                pair.clock.advance(Duration::from_secs(1));
                tokio::task::yield_now().await;
            }
        };
        tokio::select! {
            result = &mut second => result,
            () = ticker => Err("the ticker ended".into()),
        }
    };
    let (first, second) = tokio::join!(first, driver);
    Ok((first?, second?))
}

#[tokio::test]
async fn concurrent_loads_of_one_role_each_read_the_revision_they_asked_for() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (first, second) = race(&origin, "HEAD", Some("v1"), Some("dev")).await?;
        assert_eq!(
            first.map_err(|e| format!("{e:?}"))?,
            (origin.commits[0].clone(), Some("model-1".to_owned()))
        );
        assert_eq!(
            second.map_err(|e| format!("{e:?}"))?,
            (origin.commits[1].clone(), Some("model-dev".to_owned()))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_first_loads_of_one_role_both_succeed() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (first, second) = race(&origin, "clone", None, None).await?;
        let first = first.map_err(|e| format!("{e:?}"))?;
        let second = second.map_err(|e| format!("{e:?}"))?;
        assert_eq!(first, second);
        assert_eq!(first.0, origin.commits[0]);
        Ok(())
    })
    .await
}
