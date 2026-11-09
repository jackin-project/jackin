// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use jackin_core::{FileSystem, HostPath, RoleName, StdFileSystem, WorkspaceName};
use jackin_image::RoleRevision;
use jackin_session::{LoadedRole, RoleCatalog, RoleError};
use jackin_testkit::{FakeClock, PausesOn, TestRunner, WorkingFs};
use tokio_util::sync::CancellationToken;

use super::hook_fs::HookFs;
use super::roles_origin::{Origin, cache, git};
use super::support::{TestResult, guard};

type Loaded = Result<(String, Option<String>), RoleError>;

fn summary(loaded: &LoadedRole) -> (String, Option<String>) {
    let RoleRevision { commit, .. } = loaded.revision();
    (
        commit.as_str().to_owned(),
        loaded
            .manifest()
            .agents
            .get(&jackin_core::Agent::Codex)
            .and_then(|s| s.model.as_ref())
            .as_ref()
            .map(|model| model.as_str().to_owned()),
    )
}

struct Pair {
    first: RoleCatalog,
    second: RoleCatalog,
    pause: Arc<PausesOn>,
    clock: Arc<FakeClock>,
    contended: Arc<AtomicUsize>,
}

impl Pair {
    async fn until_second_waits(&self) {
        while self.contended.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    }
}

fn pair(origin: &Origin, cache: &HostPath, word: &'static str) -> Pair {
    let runner = Arc::new(TestRunner::with_home(&origin.home));
    let pause = Arc::new(PausesOn::new(&runner, word));
    let clock = Arc::new(FakeClock::new());
    let fs: Arc<dyn FileSystem> =
        Arc::new(WorkingFs::new(Arc::new(StdFileSystem::default()), &clock));
    let contended = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&contended);
    let waiting: Arc<dyn FileSystem> = Arc::new(HookFs::new(
        Arc::clone(&fs),
        Box::new(|_| Ok(())),
        Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }),
    ));
    Pair {
        first: RoleCatalog::new(
            Arc::clone(&fs),
            Arc::<PausesOn>::clone(&pause),
            Arc::<FakeClock>::clone(&clock),
            cache.clone(),
        ),
        second: RoleCatalog::new(
            waiting,
            runner,
            Arc::<FakeClock>::clone(&clock),
            cache.clone(),
        ),
        pause,
        clock,
        contended,
    }
}

async fn load(
    catalog: &RoleCatalog,
    origin: &Origin,
    rev: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Loaded, Box<dyn std::error::Error>> {
    let config = origin.config(rev)?;
    let workspace = config
        .workspace(&WorkspaceName::parse("w")?)
        .ok_or("no workspace")?;
    let name = RoleName::parse("sentinel")?;
    Ok(catalog
        .load(&config, Some(&name), workspace, cancel)
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
    let never = CancellationToken::new();
    let first = load(&pair.first, origin, first_rev, &never);
    let driver = async {
        pair.pause.reached().await;
        let second = load(&pair.second, origin, second_rev, &never);
        tokio::pin!(second);
        let early = tokio::select! {
            biased;
            result = &mut second => Some(result),
            () = pair.until_second_waits() => None,
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
            (origin.commits[0].clone(), Some("gpt-6.1-sol".to_owned()))
        );
        assert_eq!(
            second.map_err(|e| format!("{e:?}"))?,
            (origin.commits[1].clone(), Some("gpt-6.1-sol".to_owned()))
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

#[tokio::test]
async fn a_load_cancelled_while_it_waits_for_the_cache_lock_changes_nothing() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let pair = pair(&origin, &cache, "HEAD");
        let never = CancellationToken::new();
        let token = CancellationToken::new();
        let first = load(&pair.first, &origin, Some("v1"), &never);
        let driver = async {
            pair.pause.reached().await;
            let third = load(&pair.second, &origin, Some("dev"), &token);
            tokio::pin!(third);
            let cancelled = tokio::select! {
                result = &mut third => result,
                () = async {
                    pair.until_second_waits().await;
                    token.cancel();
                } => third.await,
            };
            let head = git(
                &cache.as_path().join("roles/sentinel"),
                &["rev-parse", "HEAD"],
            )
            .await;
            pair.pause.release();
            (cancelled, head)
        };
        let (first, (third, head)) = tokio::join!(first, driver);
        let third = third?;
        assert!(matches!(third, Err(RoleError::Cancelled)), "{third:?}");
        assert_eq!(head?, origin.commits[0]);
        assert_eq!(
            first?.map_err(|e| format!("{e:?}"))?,
            (origin.commits[0].clone(), Some("gpt-6.1-sol".to_owned()))
        );
        Ok(())
    })
    .await
}
