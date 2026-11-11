// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{BuilderName, ImageId, ImageTag};
use jackin_image::{BuildStage, ContextSource, EngineError, ImageEngine, ImageEvent, PlannedBuild};
use jackin_testkit::{BuildEffect, EngineMoment, FakeEngine};
use tokio_util::sync::CancellationToken;

type TestResult = Result<(), Box<dyn Error>>;

fn tag(name: &str) -> Result<ImageTag, Box<dyn Error>> {
    Ok(ImageTag::parse(&format!(
        "jackin.invalid/{name}:0123456789abcdef"
    ))?)
}

fn planned(stage: BuildStage, name: &str) -> Result<PlannedBuild, Box<dyn Error>> {
    Ok(PlannedBuild {
        stage,
        tag: tag(name)?,
        args: Vec::new(),
        context: ContextSource::Materialized(Vec::new()),
        stdin: None,
    })
}

#[tokio::test]
async fn a_pre_cancelled_build_is_cancelled_before_it_fails_prints_or_stores_anything() -> TestResult
{
    let engine = FakeEngine::new();
    engine.fail_next(BuildStage::Construct);
    engine.script_lines(BuildStage::Construct, vec!["a line".to_owned()]);
    let build = planned(BuildStage::Construct, "construct")?;
    let (sink, mut received) = tokio::sync::mpsc::channel::<ImageEvent>(8);

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let result = engine.build(&build, &sink, &cancelled).await;
    assert!(
        matches!(result, Err(EngineError::Cancelled { group: None })),
        "{result:?}"
    );
    assert!(engine.images().is_empty());
    assert!(engine.builds().is_empty());
    assert!(matches!(
        received.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));

    let live = CancellationToken::new();
    let failed = engine.build(&build, &sink, &live).await;
    assert!(
        matches!(
            failed,
            Err(EngineError::BuildFailed {
                stage: BuildStage::Construct,
                ..
            })
        ),
        "{failed:?}"
    );
    let built = engine.build(&build, &sink, &live).await;
    assert!(built.is_ok(), "{built:?}");
    assert_eq!(engine.builds().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_scripted_find_failure_hits_one_call_of_one_tag_and_then_clears() -> TestResult {
    let engine = FakeEngine::new();
    let (hit, other) = (tag("hit")?, tag("other")?);
    engine.at(EngineMoment::FindFails(hit.clone()));
    assert!(matches!(
        engine.find(&other, &CancellationToken::new()).await,
        Ok(None)
    ));
    let failed = engine.find(&hit, &CancellationToken::new()).await;
    assert!(
        matches!(&failed, Err(EngineError::DigestUnreadable { tag, .. }) if *tag == hit),
        "{failed:?}"
    );
    assert!(matches!(
        engine.find(&hit, &CancellationToken::new()).await,
        Ok(None)
    ));
    Ok(())
}

#[tokio::test]
async fn a_scripted_builder_check_failure_hits_one_call_and_then_clears() -> TestResult {
    let engine = FakeEngine::new();
    let builder = BuilderName::parse("jackin-builder")?;
    engine
        .check_builder(&builder, &CancellationToken::new())
        .await?;
    engine.at(EngineMoment::CheckBuilderFails);
    let failed = engine
        .check_builder(&builder, &CancellationToken::new())
        .await;
    assert!(
        matches!(&failed, Err(EngineError::BuilderUnreadable { builder: named, .. }) if *named == builder),
        "{failed:?}"
    );
    engine
        .check_builder(&builder, &CancellationToken::new())
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_cancel_at_the_end_of_a_build_still_returns_the_image_it_stored() -> TestResult {
    let engine = FakeEngine::new();
    engine.at(EngineMoment::BuildFinished {
        stage: BuildStage::Construct,
        effect: BuildEffect::Cancel,
    });
    let build = planned(BuildStage::Construct, "construct")?;
    let (sink, _received) = tokio::sync::mpsc::channel::<ImageEvent>(8);
    let token = CancellationToken::new();
    let id = engine.build(&build, &sink, &token).await?;
    assert!(token.is_cancelled());
    assert_eq!(engine.images().get(&build.tag), Some(&id));
    Ok(())
}

const GUARD: Duration = Duration::from_secs(10);

async fn until_waiters(engine: &FakeEngine, key: &ImageTag, waiters: usize) -> TestResult {
    let waiting = async {
        while engine.waiters(key) != waiters {
            tokio::task::yield_now().await;
        }
    };
    tokio::time::timeout(GUARD, waiting).await.map_err(|_| {
        format!(
            "waiting for {waiters} waiters of {key}; saw {}",
            engine.waiters(key)
        )
    })?;
    Ok(())
}

fn hold_in_task(
    engine: &Arc<FakeEngine>,
    key: &ImageTag,
    cancel: &CancellationToken,
) -> tokio::task::JoinHandle<Result<Option<jackin_core::FileLock>, EngineError>> {
    let (engine, key, cancel) = (Arc::clone(engine), key.clone(), cancel.clone());
    tokio::spawn(async move { engine.hold(&key, &cancel).await })
}

#[tokio::test]
async fn a_second_hold_of_one_tag_waits_until_the_first_is_dropped_and_other_tags_do_not_wait()
-> TestResult {
    tokio::time::timeout(GUARD, async {
        let engine = Arc::new(FakeEngine::new());
        let (key, other) = (tag("final")?, tag("other")?);
        let cancel = CancellationToken::new();
        let first = engine
            .hold(&key, &cancel)
            .await?
            .ok_or("the fake engine granted no lock")?;
        assert_eq!(engine.waiters(&key), 0);

        let different = engine.hold(&other, &cancel).await?;
        assert!(different.is_some(), "a different key waited for the first");
        assert_eq!((engine.waiters(&key), engine.waiters(&other)), (0, 0));

        let second = hold_in_task(&engine, &key, &cancel);
        until_waiters(&engine, &key, 1).await?;
        assert!(!second.is_finished(), "the second hold did not wait");
        drop(first);
        let second = second.await??;
        assert!(second.is_some());
        assert_eq!(engine.waiters(&key), 0);
        Ok(())
    })
    .await
    .map_err(|_| "the test hung")?
}

#[tokio::test]
async fn a_waiting_hold_honors_its_cancel_and_changes_nothing_for_the_holder() -> TestResult {
    tokio::time::timeout(GUARD, async {
        let engine = Arc::new(FakeEngine::new());
        let key = tag("final")?;
        let holder = CancellationToken::new();
        let waiter = CancellationToken::new();
        let first = engine
            .hold(&key, &holder)
            .await?
            .ok_or("the fake engine granted no lock")?;

        let cancelled = hold_in_task(&engine, &key, &waiter);
        until_waiters(&engine, &key, 1).await?;
        waiter.cancel();
        let result = cancelled.await?;
        assert!(
            matches!(result, Err(EngineError::Cancelled { group: None })),
            "{result:?}"
        );
        assert_eq!(engine.waiters(&key), 0);

        let again = hold_in_task(&engine, &key, &holder);
        until_waiters(&engine, &key, 1).await?;
        assert!(
            !again.is_finished(),
            "the cancel released the holder's lock"
        );
        drop(first);
        assert!(again.await??.is_some());
        Ok(())
    })
    .await
    .map_err(|_| "the test hung")?
}

fn other_id() -> Result<ImageId, Box<dyn Error>> {
    Ok(ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?)
}

#[tokio::test]
async fn a_new_id_effect_changes_the_id_of_the_next_build_of_its_stage_only() -> TestResult {
    let engine = FakeEngine::new();
    let (sink, _received) = tokio::sync::mpsc::channel::<ImageEvent>(8);
    let token = CancellationToken::new();
    let derived = planned(BuildStage::Derived, "derived")?;
    let role = planned(BuildStage::Role, "role")?;
    let original = engine.build(&derived, &sink, &token).await?;
    assert_eq!(engine.build(&derived, &sink, &token).await?, original);

    let other = other_id()?;
    assert_ne!(original, other);
    engine.at(EngineMoment::BuildFinished {
        stage: BuildStage::Derived,
        effect: BuildEffect::NewId(other.clone()),
    });
    let untouched = engine.build(&role, &sink, &token).await?;
    assert_ne!(untouched, other, "another stage took the effect");
    assert_eq!(
        engine.find(&derived.tag, &CancellationToken::new()).await?,
        Some(original.clone())
    );

    let rebuilt = engine.build(&derived, &sink, &token).await?;
    assert_eq!(rebuilt, other);
    assert_eq!(
        engine.find(&derived.tag, &CancellationToken::new()).await?,
        Some(other)
    );
    assert_eq!(engine.build(&derived, &sink, &token).await?, original);
    assert_eq!(
        engine.find(&derived.tag, &CancellationToken::new()).await?,
        Some(original)
    );
    Ok(())
}
