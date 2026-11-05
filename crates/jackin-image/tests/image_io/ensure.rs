// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    DockerSubcommand, GroupFault, ImageId, ImageTag, OutputStream, ProcessError, ProcessRunner,
    ProcessStatus, StdFileSystem,
};
use jackin_image::{
    BuildStage, BuildxEngine, BuiltImage, DockerfileError, EngineError, ImageEngine, ImageError,
    ImageEvent, ImagePlan, ImageRequest, ImageSource, Rebuild, ensure_image,
};
use jackin_testkit::{
    BuildEffect, EngineMoment, FakeEngine, ProcessReply, Retag, ScriptedProcessRunner,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{
    AfterBuild, Delivery, Fixture, Outcome, Script, Stack, TestResult, assert_local_only, drain,
    source, tag_id, until_role_line,
};

async fn ensure(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
) -> Result<(Result<BuiltImage, ImageError>, Vec<ImageEvent>), Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel(256);
    let result = ensure_image(engine, request, &tx, &CancellationToken::new()).await;
    Ok((result, drain(&mut rx)))
}

fn started(stage: BuildStage) -> [ImageEvent; 2] {
    [
        ImageEvent::StageStarted(stage),
        ImageEvent::StageFinished(stage),
    ]
}

async fn scenario(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
    built: &dyn Fn() -> Vec<String>,
) -> TestResult {
    let plan = ImagePlan::new(request)?;
    let (first, events) = ensure(engine, request).await?;
    let first = first?;
    assert_eq!(first.source, ImageSource::Built);
    let construct_tag = plan.construct_build().tag.clone();
    let construct_id = engine
        .find(&construct_tag, &CancellationToken::new())
        .await?
        .ok_or("the construct image is missing")?;
    let finals = plan.final_builds(&construct_id);
    let order = [&construct_tag, &finals.role.tag, &finals.derived.tag].map(ToString::to_string);
    assert_eq!(built(), order, "built tags in order");
    assert_eq!(first.tag, plan.final_tag(&construct_id));
    assert_eq!(first.tag, finals.derived.tag);
    assert_eq!(
        engine.find(&first.tag, &CancellationToken::new()).await?,
        Some(first.id.clone())
    );
    assert_ne!(
        first.id, construct_id,
        "the derived ID is not the construct ID"
    );
    let expected: Vec<ImageEvent> = [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
        .into_iter()
        .flat_map(started)
        .collect();
    assert_eq!(events, expected, "events of the first call");

    let (second, events) = ensure(engine, request).await?;
    let second = second?;
    assert_eq!(second.source, ImageSource::Cache);
    assert_eq!((second.tag, second.id), (first.tag, first.id));
    assert_eq!(built().len(), 3, "the second call built nothing");
    assert_eq!(
        events,
        [
            ImageEvent::StageCached(BuildStage::Construct),
            ImageEvent::StageCached(BuildStage::Derived)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn ensure_image_builds_in_order_then_serves_the_second_call_from_cache_on_the_fake_engine()
-> TestResult {
    guard(async {
        let request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        let built = || {
            engine
                .builds()
                .iter()
                .map(|record| record.tag.to_string())
                .collect()
        };
        scenario(&engine, &request, &built).await
    })
    .await
}

#[tokio::test]
async fn ensure_image_builds_in_order_then_serves_the_second_call_from_cache_on_buildx_over_scripted_docker()
-> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let built = || stack.built_tags();
        scenario(&stack.engine, &request, &built).await?;
        assert_local_only(&stack.runner)
    })
    .await
}

fn stages(engine: &FakeEngine, from: usize) -> Vec<BuildStage> {
    engine.builds()[from..]
        .iter()
        .map(|record| record.stage)
        .collect()
}

#[tokio::test]
async fn a_changed_construct_id_rebuilds_role_and_derived_only_and_rebuild_forces_all() -> TestResult
{
    guard(async {
        let mut request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        let plan = ImagePlan::new(&request)?;
        let (warm, _) = ensure(&engine, &request).await?;
        let warm = warm?;
        let construct_tag = plan.construct_build().tag.clone();
        let replacement = ImageId::parse(&tag_id("replacement"))?;
        engine.pin_id(&construct_tag, replacement.clone());
        engine
            .attest(&construct_tag, &replacement, &CancellationToken::new())
            .await?;
        let before = engine.builds().len();
        let (changed, events) = ensure(&engine, &request).await?;
        let changed = changed?;
        assert_eq!(changed.source, ImageSource::Built);
        assert_eq!(
            stages(&engine, before),
            [BuildStage::Role, BuildStage::Derived]
        );
        let expected: Vec<ImageEvent> = [ImageEvent::StageCached(BuildStage::Construct)]
            .into_iter()
            .chain(started(BuildStage::Role))
            .chain(started(BuildStage::Derived))
            .collect();
        assert_eq!(events, expected);
        assert_ne!(
            changed.tag, warm.tag,
            "the final tag follows the construct ID"
        );
        assert!(
            engine.images().contains_key(&warm.tag),
            "the old image stays"
        );

        request.rebuild = Rebuild::Yes;
        let before = engine.builds().len();
        let (forced, events) = ensure(&engine, &request).await?;
        assert_eq!(forced?.source, ImageSource::Built);
        assert_eq!(
            stages(&engine, before),
            [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
        );
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, ImageEvent::StageCached(_))),
            "{events:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_role_build_stops_before_derived_and_the_next_call_reuses_the_construct()
-> TestResult {
    guard(async {
        let request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        engine.fail_next(BuildStage::Role);
        let (failed, events) = ensure(&engine, &request).await?;
        match failed {
            Err(ImageError::Engine(EngineError::BuildFailed { stage, .. })) => {
                assert_eq!(stage, BuildStage::Role);
            }
            other => return Err(format!("expected a role failure, got {other:?}").into()),
        }
        assert_eq!(stages(&engine, 0), [BuildStage::Construct]);
        let expected: Vec<ImageEvent> = started(BuildStage::Construct)
            .into_iter()
            .chain([ImageEvent::StageStarted(BuildStage::Role)])
            .collect();
        assert_eq!(events, expected, "events end at the started role stage");

        let (retry, events) = ensure(&engine, &request).await?;
        assert_eq!(retry?.source, ImageSource::Built);
        assert_eq!(
            stages(&engine, 0),
            [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
        );
        assert_eq!(
            events.first(),
            Some(&ImageEvent::StageCached(BuildStage::Construct))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ensure_image_rejects_a_role_dockerfile_without_construct_before_any_engine_call()
-> TestResult {
    guard(async {
        let mut request = Fixture::new()?.request()?;
        request.dockerfile = source("Dockerfile", b"FROM ubuntu\n")?;
        let engine = FakeEngine::new();
        let (result, events) = ensure(&engine, &request).await?;
        match result {
            Err(ImageError::Dockerfile(DockerfileError::FinalStageNotConstruct {
                line,
                found,
            })) => {
                assert_eq!((line, found.as_str()), (1, "ubuntu"));
            }
            other => return Err(format!("expected FinalStageNotConstruct, got {other:?}").into()),
        }
        assert!(engine.images().is_empty(), "images");
        assert!(engine.builds().is_empty(), "builds");
        assert!(events.is_empty(), "{events:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_builder_is_checked_once_before_the_first_build_and_never_on_a_cache_hit() -> TestResult
{
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        stack.model.driver("docker-container");
        let (result, _) = ensure(&stack.engine, &request).await?;
        match result {
            Err(ImageError::Engine(EngineError::BuilderDriver { builder, driver })) => {
                assert_eq!(builder, request.builder);
                assert_eq!(driver, "docker-container");
            }
            other => return Err(format!("part A: got {other:?}").into()),
        }
        assert!(
            stack.calls(DockerSubcommand::BuildxBuild).is_empty(),
            "part A built"
        );
        assert!(stack.model.images().is_empty(), "part A left an image");

        let hit = Stack::new()?;
        hit.model.driver("docker-container");
        let request = hit.fixture.request()?;
        let plan = ImagePlan::new(&request)?;
        let construct = ImageId::parse(&hit.model.preload(plan.construct_build().tag.as_str()))?;
        hit.engine
            .attest(
                &plan.construct_build().tag,
                &construct,
                &CancellationToken::new(),
            )
            .await?;
        let derived = ImageId::parse(&hit.model.preload(plan.final_tag(&construct).as_str()))?;
        hit.engine
            .attest(
                &plan.final_tag(&construct),
                &derived,
                &CancellationToken::new(),
            )
            .await?;
        let (result, _) = ensure(&hit.engine, &request).await?;
        assert_eq!(result?.source, ImageSource::Cache);
        assert!(
            hit.calls(DockerSubcommand::BuildxInspect).is_empty(),
            "part B inspected"
        );
        assert!(
            hit.calls(DockerSubcommand::BuildxBuild).is_empty(),
            "part B built"
        );

        let cold = Stack::new()?;
        let request = cold.fixture.request()?;
        let (result, _) = ensure(&cold.engine, &request).await?;
        assert_eq!(result?.source, ImageSource::Built);
        assert_eq!(
            cold.calls(DockerSubcommand::BuildxInspect).len(),
            1,
            "part C inspects"
        );
        let seen = cold.runner.seen();
        let position = |sub| {
            seen.iter()
                .position(|call| call.program == jackin_core::Program::Docker(sub))
        };
        assert!(
            position(DockerSubcommand::BuildxInspect) < position(DockerSubcommand::BuildxBuild),
            "the check precedes the first build"
        );
        Ok(())
    })
    .await
}

fn role_lines(texts: &[(OutputStream, &str)]) -> Script {
    Script {
        lines: texts
            .iter()
            .map(|(stream, text)| (*stream, (*text).to_owned()))
            .collect(),
        ..Script::default()
    }
}

#[tokio::test]
async fn build_output_reaches_the_sink_between_stage_events_and_a_failure_stops_the_pipeline()
-> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        stack.model.script(
            BuildStage::Role,
            role_lines(&[
                (OutputStream::Stdout, "\x1b[32mstep one\x1b[0m"),
                (OutputStream::Stderr, "step two"),
                (OutputStream::Stdout, "step three"),
            ]),
        );
        let (result, events) = ensure(&stack.engine, &request).await?;
        result?;
        let start = events
            .iter()
            .position(|event| *event == ImageEvent::StageStarted(BuildStage::Role))
            .ok_or("no role start")?;
        let texts: Vec<String> = events[start + 1..start + 4]
            .iter()
            .map(|event| match event {
                ImageEvent::Line {
                    stage: BuildStage::Role,
                    text,
                } => text.as_str().to_owned(),
                other => format!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(texts, ["step one", "step two", "step three"]);
        assert_eq!(
            events[start + 4],
            ImageEvent::StageFinished(BuildStage::Role)
        );

        let failing = Stack::new()?;
        let request = failing.fixture.request()?;
        let script = Script {
            outcome: Outcome::Exit(ProcessStatus::Exited(2)),
            ..role_lines(&[(OutputStream::Stderr, "\x1b[31merror\x1b[0m: boom")])
        };
        failing.model.script(BuildStage::Role, script);
        let (result, events) = ensure(&failing.engine, &request).await?;
        match result {
            Err(ImageError::Engine(EngineError::BuildFailed {
                stage,
                status,
                tail,
            })) => {
                assert_eq!(stage, BuildStage::Role);
                assert_eq!(status, ProcessStatus::Exited(2));
                assert_eq!(tail.as_str(), "error: boom");
            }
            other => return Err(format!("expected BuildFailed, got {other:?}").into()),
        }
        let plan = ImagePlan::new(&request)?;
        let construct = ImageId::parse(&tag_id(plan.construct_build().tag.as_str()))?;
        let derived = plan.final_tag(&construct).to_string();
        assert!(
            !failing.built_tags().contains(&derived),
            "the derived image was built"
        );
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Role)),
            "{events:?}"
        );
        Ok(())
    })
    .await
}

async fn cancel_at_first_line(
    rx: &mut mpsc::Receiver<ImageEvent>,
    cancel: &CancellationToken,
) -> Vec<ImageEvent> {
    let mut seen = Vec::new();
    while let Some(event) = rx.recv().await {
        let is_line = matches!(event, ImageEvent::Line { .. });
        seen.push(event);
        if is_line {
            cancel.cancel();
            break;
        }
    }
    seen
}

#[tokio::test]
async fn cancel_during_a_build_returns_cancelled_and_stops_the_pipeline() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let script = Script {
            outcome: Outcome::UntilCancelled,
            ..role_lines(&[(OutputStream::Stdout, "first")])
        };
        stack.model.script(BuildStage::Role, script);
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let cancel = CancellationToken::new();
        let run = ensure_image(&stack.engine, &request, &tx, &cancel);
        let driver = cancel_at_first_line(&mut rx, &cancel);
        let (result, mut events) = tokio::join!(run, driver);
        events.extend(drain(&mut rx));
        assert!(
            matches!(result, Err(ImageError::Cancelled { .. })),
            "{result:?}"
        );
        let plan = ImagePlan::new(&request)?;
        let construct = ImageId::parse(&tag_id(plan.construct_build().tag.as_str()))?;
        let derived = plan.final_tag(&construct).to_string();
        assert!(
            !stack.built_tags().contains(&derived),
            "the derived image was built"
        );
        assert!(
            stack.fixture.work_entries().is_empty(),
            "directory left behind"
        );
        assert_ne!(
            events.last(),
            Some(&ImageEvent::StageFinished(BuildStage::Role))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_built_image_must_be_present_in_the_engine_under_its_final_tag() -> TestResult {
    guard(async {
        let other = tag_id("someone else");
        let rows = [
            ("present", Delivery::Normal),
            ("dropped", Delivery::Dropped),
            ("moved", Delivery::Moved(other.clone())),
        ];
        for (label, delivery) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let plan = ImagePlan::new(&request)?;
            let construct = ImageId::parse(&tag_id(plan.construct_build().tag.as_str()))?;
            let tag = plan.final_tag(&construct);
            let built = ImageId::parse(&tag_id(tag.as_str()))?;
            stack.model.delivery(delivery);
            let (result, _) = ensure(&stack.engine, &request).await?;
            match (label, result) {
                ("present", Ok(image)) => {
                    assert_eq!(image.source, ImageSource::Built);
                    assert_eq!((image.tag, image.id), (tag, built));
                }
                (
                    "dropped",
                    Err(ImageError::NotDelivered {
                        tag: found,
                        built: id,
                        found: none,
                    }),
                ) => {
                    assert_eq!((found, id, none), (tag, built, None));
                }
                (
                    "moved",
                    Err(ImageError::NotDelivered {
                        found: Some(found),
                        built: id,
                        ..
                    }),
                ) => {
                    assert_eq!((found, id), (ImageId::parse(&other)?, built));
                }
                (_, other) => return Err(format!("{label}: got {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

fn id_of(tag: &ImageTag) -> Result<ImageId, Box<dyn Error>> {
    Ok(ImageId::parse(&tag_id(tag.as_str()))?)
}

struct Tags {
    construct: ImageTag,
    role: ImageTag,
    derived: ImageTag,
    construct_id: ImageId,
    role_id: ImageId,
    derived_id: ImageId,
}

impl Tags {
    fn of(request: &ImageRequest) -> Result<Tags, Box<dyn Error>> {
        let plan = ImagePlan::new(request)?;
        let construct = plan.construct_build().tag.clone();
        let construct_id = id_of(&construct)?;
        let finals = plan.final_builds(&construct_id);
        Ok(Tags {
            role_id: id_of(&finals.role.tag)?,
            derived_id: id_of(&finals.derived.tag)?,
            role: finals.role.tag,
            derived: finals.derived.tag,
            construct,
            construct_id,
        })
    }
}

fn moved(
    result: Result<BuiltImage, ImageError>,
) -> Result<(ImageTag, ImageId, Option<ImageId>), Box<dyn Error>> {
    match result {
        Err(ImageError::SourceMoved {
            tag,
            expected,
            found,
        }) => Ok((tag, expected, found)),
        other => Err(format!("expected SourceMoved, got {other:?}").into()),
    }
}

fn retag(tag: &ImageTag, to: Option<ImageId>) -> Retag {
    Retag {
        tag: tag.clone(),
        to,
    }
}

fn built_stages(engine: &FakeEngine) -> Vec<BuildStage> {
    stages(engine, 0)
}

#[tokio::test]
async fn a_source_tag_that_moved_before_a_dependent_build_stops_the_pipeline_before_that_build_starts()
-> TestResult {
    guard(async {
        let other = ImageId::parse(&tag_id("someone else"))?;

        let mut request = Fixture::new()?.request()?;
        let tags = Tags::of(&request)?;
        let engine = FakeEngine::new();
        engine.at(EngineMoment::AfterFind {
            tag: tags.derived.clone(),
            retag: retag(&tags.construct, Some(other.clone())),
        });
        let (result, events) = ensure(&engine, &request).await?;
        assert_eq!(
            moved(result)?,
            (
                tags.construct.clone(),
                tags.construct_id.clone(),
                Some(other.clone())
            ),
            "cold construct"
        );
        assert_eq!(built_stages(&engine), [BuildStage::Construct]);
        assert!(
            !events.contains(&ImageEvent::StageStarted(BuildStage::Role)),
            "{events:?}"
        );
        assert!(!engine.images().contains_key(&tags.role), "role tag stored");

        let warm = FakeEngine::new();
        let (first, _) = ensure(&warm, &request).await?;
        first?;
        let hook = request.hooks.setup.as_mut().ok_or("no setup hook")?;
        hook.bytes = b"#!/bin/sh\necho changed\n".to_vec();
        let changed = Tags::of(&request)?;
        assert_ne!(changed.derived, tags.derived, "the final tag must miss");
        warm.at(EngineMoment::AfterFind {
            tag: changed.derived.clone(),
            retag: retag(&tags.construct, None),
        });
        let (result, events) = ensure(&warm, &request).await?;
        assert_eq!(
            moved(result)?,
            (tags.construct.clone(), tags.construct_id.clone(), None),
            "cached construct"
        );
        assert_eq!(built_stages(&warm).len(), 3, "no new build");
        assert!(
            !events.contains(&ImageEvent::StageStarted(BuildStage::Role)),
            "{events:?}"
        );
        assert!(
            !warm.images().contains_key(&changed.role),
            "role tag stored"
        );

        let request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        engine.at(EngineMoment::BuildFinished {
            stage: BuildStage::Role,
            effect: BuildEffect::Retag(retag(&tags.role, Some(other.clone()))),
        });
        let (result, events) = ensure(&engine, &request).await?;
        assert_eq!(
            moved(result)?,
            (tags.role.clone(), tags.role_id.clone(), Some(other)),
            "role stage"
        );
        assert_eq!(
            built_stages(&engine),
            [BuildStage::Construct, BuildStage::Role]
        );
        assert!(
            events.contains(&ImageEvent::StageFinished(BuildStage::Role)),
            "{events:?}"
        );
        assert!(
            !events.contains(&ImageEvent::StageStarted(BuildStage::Derived)),
            "{events:?}"
        );
        assert!(
            !engine.images().contains_key(&tags.derived),
            "derived tag stored"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_source_tag_that_moved_during_a_dependent_build_fails_that_stage_and_starts_nothing_after_it()
-> TestResult {
    guard(async {
        let other = ImageId::parse(&tag_id("someone else"))?;
        let request = Fixture::new()?.request()?;
        let tags = Tags::of(&request)?;

        let engine = FakeEngine::new();
        engine.at(EngineMoment::BuildFinished {
            stage: BuildStage::Role,
            effect: BuildEffect::Retag(retag(&tags.construct, Some(other.clone()))),
        });
        let (result, events) = ensure(&engine, &request).await?;
        assert_eq!(
            moved(result)?,
            (
                tags.construct.clone(),
                tags.construct_id.clone(),
                Some(other.clone())
            ),
            "role build"
        );
        assert_eq!(
            built_stages(&engine),
            [BuildStage::Construct, BuildStage::Role]
        );
        assert!(
            events.contains(&ImageEvent::StageStarted(BuildStage::Role)),
            "{events:?}"
        );
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Role)),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, ImageEvent::StageStarted(BuildStage::Derived))),
            "{events:?}"
        );

        let engine = FakeEngine::new();
        engine.at(EngineMoment::BuildFinished {
            stage: BuildStage::Derived,
            effect: BuildEffect::Retag(retag(&tags.role, None)),
        });
        let (result, events) = ensure(&engine, &request).await?;
        assert_eq!(
            moved(result)?,
            (tags.role.clone(), tags.role_id.clone(), None),
            "derived build"
        );
        assert_eq!(engine.images().get(&tags.derived), Some(&tags.derived_id));
        assert!(
            events.contains(&ImageEvent::StageStarted(BuildStage::Derived)),
            "{events:?}"
        );
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Derived)),
            "{events:?}"
        );

        let engine = FakeEngine::new();
        engine.at(EngineMoment::BuildFinished {
            stage: BuildStage::Derived,
            effect: BuildEffect::Retag(retag(&tags.construct, Some(other.clone()))),
        });
        let (result, events) = ensure(&engine, &request).await?;
        assert_eq!(
            moved(result)?,
            (
                tags.construct.clone(),
                tags.construct_id.clone(),
                Some(other.clone())
            ),
            "derived build construct fence"
        );
        assert_eq!(
            engine.images().get(&tags.construct),
            Some(&other),
            "the retag ran"
        );
        assert!(
            events.contains(&ImageEvent::StageStarted(BuildStage::Derived)),
            "{events:?}"
        );
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Derived)),
            "{events:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn source_checks_read_image_inspect_and_fail_closed() -> TestResult {
    guard(async {
        let other = tag_id("someone else");

        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let tags = Tags::of(&request)?;
        stack.model.after_build(
            BuildStage::Role,
            AfterBuild::Retag {
                tag: tags.construct.to_string(),
                id: other.clone(),
            },
        );
        let (result, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(
            moved(result)?,
            (
                tags.construct.clone(),
                tags.construct_id.clone(),
                Some(ImageId::parse(&other)?)
            ),
            "retagged construct"
        );
        let derived = tags.derived.to_string();
        assert!(
            !stack.built_tags().contains(&derived),
            "the derived build ran"
        );
        assert!(
            stack.fixture.work_entries().is_empty(),
            "directory left behind"
        );
        assert_local_only(&stack.runner)?;

        let stack = Stack::new()?;
        stack.model.after_build(
            BuildStage::Construct,
            AfterBuild::FailInspect {
                tag: tags.construct.to_string(),
            },
        );
        let (result, _) = ensure(&stack.engine, &request).await?;
        match result {
            Err(ImageError::Engine(EngineError::DigestUnreadable { tag, output })) => {
                assert_eq!(tag, tags.construct);
                assert!(
                    format!("{output:?}").contains("Cannot connect to the Docker daemon"),
                    "{output:?}"
                );
            }
            other => return Err(format!("expected DigestUnreadable, got {other:?}").into()),
        }
        assert_eq!(stack.built_tags(), [tags.construct.to_string()]);
        assert!(
            stack.fixture.work_entries().is_empty(),
            "directory left behind"
        );
        assert_local_only(&stack.runner)
    })
    .await
}

#[tokio::test]
async fn a_cancelled_token_stops_ensure_image_before_any_engine_work_and_between_stages()
-> TestResult {
    guard(async {
        let mut request = Fixture::new()?.request()?;
        for rebuild in [Rebuild::No, Rebuild::Yes] {
            request.rebuild = rebuild;
            let engine = FakeEngine::new();
            let (tx, mut rx) = mpsc::channel(256);
            let cancel = CancellationToken::new();
            cancel.cancel();
            let result = ensure_image(&engine, &request, &tx, &cancel).await;
            assert!(
                matches!(result, Err(ImageError::Cancelled { .. })),
                "{rebuild:?}: {result:?}"
            );
            assert!(engine.builds().is_empty(), "{rebuild:?}: builds");
            assert!(engine.images().is_empty(), "{rebuild:?}: images");
            assert!(drain(&mut rx).is_empty(), "{rebuild:?}: events");
            let (control, _) = ensure(&engine, &request).await?;
            assert_eq!(control?.source, ImageSource::Built, "{rebuild:?}: control");
            assert_eq!(engine.builds().len(), 3, "{rebuild:?}: control builds");
        }

        request.rebuild = Rebuild::No;
        let tags = Tags::of(&request)?;
        let engine = FakeEngine::new();
        engine.at(EngineMoment::BuildFinished {
            stage: BuildStage::Construct,
            effect: BuildEffect::Cancel,
        });
        let (tx, mut rx) = mpsc::channel(256);
        let result = ensure_image(&engine, &request, &tx, &CancellationToken::new()).await;
        assert!(
            matches!(result, Err(ImageError::Cancelled { .. })),
            "{result:?}"
        );
        assert_eq!(built_stages(&engine), [BuildStage::Construct]);
        assert!(!engine.images().contains_key(&tags.role), "role tag stored");
        assert!(
            !engine.images().contains_key(&tags.derived),
            "derived tag stored"
        );
        let events = drain(&mut rx);
        assert!(
            events.iter().all(|event| matches!(
                event,
                ImageEvent::StageStarted(BuildStage::Construct)
                    | ImageEvent::StageCached(BuildStage::Construct)
                    | ImageEvent::StageFinished(BuildStage::Construct)
                    | ImageEvent::Line {
                        stage: BuildStage::Construct,
                        ..
                    }
            )),
            "{events:?}"
        );
        Ok(())
    })
    .await
}

fn staged_copies(dir: &Path, found: &mut Vec<PathBuf>) -> TestResult {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            staged_copies(&path, found)?;
        } else if path.file_name().is_some_and(|name| name == "a.rs")
            || fs::read(&path).is_ok_and(|bytes| bytes == b"fn a() {}\n")
        {
            found.push(path);
        }
    }
    Ok(())
}

#[tokio::test]
async fn dropping_ensure_image_mid_build_leaves_no_copy_of_the_context() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        stack.model.script(
            BuildStage::Role,
            Script {
                lines: vec![(OutputStream::Stdout, "building".to_owned())],
                outcome: Outcome::UntilCancelled,
                ..Script::default()
            },
        );
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let cancel = CancellationToken::new();
        let mut call = Box::pin(ensure_image(&stack.engine, &request, &tx, &cancel));
        until_role_line(&mut call, &mut rx).await?;
        let mut during = Vec::new();
        staged_copies(stack.fixture.work.as_path(), &mut during)?;
        assert!(!during.is_empty(), "the control found no staged copy");
        drop(call);
        let mut after = Vec::new();
        staged_copies(stack.fixture.work.as_path(), &mut after)?;
        assert!(after.is_empty(), "copies left after the drop: {after:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_whose_group_stop_failed_reaches_the_image_error_with_its_fault() -> TestResult {
    guard(async {
        let alive = GroupFault::Alive {
            after: Duration::from_secs(5),
        };
        let rows: Vec<(&str, ProcessReply)> = vec![
            ("plain cancel", ProcessReply::cancelled()),
            (
                "cancel with a fault",
                ProcessReply::cancelled_with(alive.clone()),
            ),
            (
                "timeout with a fault",
                ProcessReply::timeout_with(alive.clone()),
            ),
        ];
        for (label, reply) in rows {
            let fixture = Fixture::new()?;
            let request = fixture.request()?;
            let runner = Arc::new(ScriptedProcessRunner::new());
            runner.expect_docker(DockerSubcommand::ImageInspect, &[], reply);
            let shared: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&runner);
            let engine = BuildxEngine::new(
                shared,
                Arc::new(StdFileSystem::default()),
                fixture.work.clone(),
            );
            let (result, _) = ensure(&engine, &request).await?;
            let shown = result.as_ref().err().map(ToString::to_string);
            match (label, result) {
                ("plain cancel", Err(ImageError::Cancelled { group: None })) => {
                    assert_eq!(shown.as_deref(), Some("cancelled"), "{label}");
                }
                ("cancel with a fault", Err(ImageError::Cancelled { group })) => {
                    assert_eq!(group, Some(alive.clone()), "{label}");
                    assert_eq!(
                        shown.as_deref(),
                        Some("cancelled; processes of its group were still alive after 5s"),
                        "{label}"
                    );
                }
                (
                    "timeout with a fault",
                    Err(ImageError::Engine(EngineError::Process(ProcessError::Timeout {
                        group,
                        ..
                    }))),
                ) => assert_eq!(group, Some(alive.clone()), "{label}"),
                (_, other) => return Err(format!("{label}: got {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_construct_tag_that_jackin_did_not_build_is_rebuilt_not_trusted() -> TestResult {
    guard(async {
        let request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        let plan = ImagePlan::new(&request)?;
        let construct_tag = plan.construct_build().tag.clone();
        let planted = ImageId::parse(&tag_id("a retagged poisoned image"))?;
        engine.plant(&construct_tag, planted.clone());

        let (result, events) = ensure(&engine, &request).await?;
        let built = result?;
        assert_eq!(built.source, ImageSource::Built);
        assert_eq!(
            stages(&engine, 0),
            [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
        );
        assert!(
            !events.contains(&ImageEvent::StageCached(BuildStage::Construct)),
            "{events:?}"
        );
        let held = engine
            .find(&construct_tag, &CancellationToken::new())
            .await?
            .ok_or("the construct tag is empty")?;
        assert_ne!(held, planted, "the planted image still holds the tag");
        assert_ne!(built.tag, plan.final_tag(&planted));

        let (again, _) = ensure(&engine, &request).await?;
        assert_eq!(again?.source, ImageSource::Cache);
        assert_eq!(stages(&engine, 0).len(), 3, "the second call built nothing");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buildx_rebuilds_a_construct_whose_image_moved_after_jackin_recorded_it() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let plan = ImagePlan::new(&request)?;
        let construct_tag = plan.construct_build().tag.clone();
        let (warm, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(warm?.source, ImageSource::Built);
        assert_eq!(stack.built_tags().len(), 3);

        let (cached, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(cached?.source, ImageSource::Cache);
        assert_eq!(stack.built_tags().len(), 3);

        stack
            .model
            .plant(construct_tag.as_str(), &tag_id("a retagged poisoned image"));
        stack.model.fresh_ids();
        let (rebuilt, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(rebuilt?.source, ImageSource::Built);
        assert_eq!(stack.built_tags().len(), 6);
        assert_eq!(
            stack.built_tags().get(3).map(String::as_str),
            Some(construct_tag.as_str())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_derived_tag_that_jackin_did_not_build_is_rebuilt_not_trusted() -> TestResult {
    guard(async {
        let request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        let plan = ImagePlan::new(&request)?;
        let (warm, _) = ensure(&engine, &request).await?;
        let warm = warm?;
        assert_eq!(warm.source, ImageSource::Built);
        let derived_tag = warm.tag.clone();

        let planted = ImageId::parse(&tag_id("a derived image that skipped the scan"))?;
        engine.plant(&derived_tag, planted.clone());
        let builds_before = engine.builds().len();

        let (result, events) = ensure(&engine, &request).await?;
        let rebuilt = result?;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_ne!(rebuilt.id, planted);
        assert_eq!(rebuilt.tag, derived_tag);
        assert_eq!(
            engine.builds().len(),
            builds_before + 2,
            "role and derived rebuilt"
        );
        assert!(
            !events.contains(&ImageEvent::StageCached(BuildStage::Derived)),
            "{events:?}"
        );
        assert_eq!(
            engine.find(&derived_tag, &CancellationToken::new()).await?,
            Some(rebuilt.id.clone())
        );
        assert_eq!(
            engine
                .attested(&derived_tag, &CancellationToken::new())
                .await?,
            Some(rebuilt.id)
        );
        assert_eq!(
            derived_tag,
            plan.final_tag(&ImageId::parse(&tag_id(
                plan.construct_build().tag.as_str()
            ))?)
        );

        let (again, _) = ensure(&engine, &request).await?;
        assert_eq!(again?.source, ImageSource::Cache);
        assert_eq!(
            engine.builds().len(),
            builds_before + 2,
            "the second call built nothing"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buildx_rebuilds_a_derived_image_that_moved_after_jackin_recorded_it() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        assert_eq!(stack.built_tags().len(), 3);

        stack.model.plant(
            warm.tag.as_str(),
            &tag_id("a derived image that skipped the scan"),
        );
        stack.model.fresh_ids();
        let (rebuilt, _) = ensure(&stack.engine, &request).await?;
        let rebuilt = rebuilt?;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_eq!(stack.built_tags().len(), 5);
        assert_eq!(
            stack.model.images().get(warm.tag.as_str()),
            Some(&rebuilt.id.to_string())
        );
        let (cached, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(cached?.source, ImageSource::Cache);
        assert_eq!(stack.built_tags().len(), 5);
        Ok(())
    })
    .await
}

type Damage = fn(&Path, &Path, &str) -> std::io::Result<()>;

#[tokio::test]
async fn buildx_does_not_trust_an_attestation_record_that_is_not_a_plain_file() -> TestResult {
    guard(async {
        let rows: [(&str, Damage); 3] = [
            ("directory", |_, record, _| fs::create_dir(record)),
            ("symlink to a valid record", |valid, record, _| {
                std::os::unix::fs::symlink(valid, record)
            }),
            ("oversized", |_, record, id| {
                fs::write(record, format!("{id}{}", " ".repeat(4096)))
            }),
        ];
        for (label, damage) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let plan = ImagePlan::new(&request)?;
            let (warm, _) = ensure(&stack.engine, &request).await?;
            warm?;
            let construct_tag = plan.construct_build().tag.clone();
            let id = stack
                .model
                .images()
                .get(construct_tag.as_str())
                .cloned()
                .ok_or("the construct image is missing")?;
            let attested = stack.fixture.work.as_path().join("attested");
            let name = jackin_core::Sha256::of(construct_tag.to_string().as_bytes());
            let record = attested.join(name.as_str());
            let valid = attested.join("valid-copy");
            fs::copy(&record, &valid)?;
            fs::remove_file(&record)?;
            damage(&valid, &record, &id)?;

            let (rebuilt, _) = ensure(&stack.engine, &request).await?;
            rebuilt?;
            assert_eq!(stack.built_tags().len(), 4, "{label}");
            assert_eq!(
                stack.built_tags().get(3).map(String::as_str),
                Some(construct_tag.as_str()),
                "{label}"
            );
            assert_eq!(
                stack
                    .engine
                    .attested(&construct_tag, &CancellationToken::new())
                    .await?
                    .map(|found| found.to_string()),
                stack.model.images().get(construct_tag.as_str()).cloned(),
                "{label}: the record was rewritten"
            );
            assert!(
                fs::symlink_metadata(&record)?.file_type().is_file(),
                "{label}"
            );
        }
        Ok(())
    })
    .await
}
