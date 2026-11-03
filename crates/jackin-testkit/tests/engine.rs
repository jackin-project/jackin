// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{BuilderName, ImageTag};
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
    assert!(matches!(engine.find(&other).await, Ok(None)));
    let failed = engine.find(&hit).await;
    assert!(
        matches!(&failed, Err(EngineError::DigestUnreadable { tag, .. }) if *tag == hit),
        "{failed:?}"
    );
    assert!(matches!(engine.find(&hit).await, Ok(None)));
    Ok(())
}

#[tokio::test]
async fn a_scripted_builder_check_failure_hits_one_call_and_then_clears() -> TestResult {
    let engine = FakeEngine::new();
    let builder = BuilderName::parse("jackin-builder")?;
    engine.check_builder(&builder).await?;
    engine.at(EngineMoment::CheckBuilderFails);
    let failed = engine.check_builder(&builder).await;
    assert!(
        matches!(&failed, Err(EngineError::BuilderUnreadable { builder: named, .. }) if *named == builder),
        "{failed:?}"
    );
    engine.check_builder(&builder).await?;
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
