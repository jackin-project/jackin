// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Clock, DockerSubcommand, GroupFault, ImageId, ImageTag, OutputStream, ProcessError,
    ProcessRunner, ProcessStatus, Sha256, StdFileSystem, TerminalText,
};
use jackin_image::{
    BuildStage, BuildxEngine, BuiltImage, ContextSource, DockerfileError, EngineError, ImageEngine,
    ImageError, ImageEvent, ImagePlan, ImageRequest, ImageSource, Rebuild, ensure_image,
};
use jackin_testkit::{DockerBuildReply, ProcessReply, ScriptedProcessRunner};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{
    DockerOnlyProcessRunner, Fixture, Outcome, Script, Stack, TestResult, drain, source, tag_id,
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

async fn planned(
    stack: &Stack,
    request: &ImageRequest,
) -> Result<(ImagePlan, jackin_image::FinalBuilds, ImageId), Box<dyn Error>> {
    let (plan, builds) = stack.plan_for(request).await?;
    let construct = stack
        .model
        .image_id(&plan.construct_build().tag)
        .ok_or("the construct image is missing")?;
    Ok((plan, builds, construct))
}

async fn attest_construct(stack: &Stack, plan: &ImagePlan, id: &ImageId) -> TestResult {
    stack
        .engine
        .attest(&plan.construct_build().tag, id, &CancellationToken::new())
        .await?;
    Ok(())
}

#[tokio::test]
async fn buildx_returns_images_with_the_resolved_parent_graph_and_reuses_a_warm_image() -> TestResult
{
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, _, _) = planned(&stack, &request).await?;
        let (first, events) = ensure(&stack.engine, &request).await?;
        let first = first?;
        assert_eq!(first.source, ImageSource::Built);
        assert_eq!(
            first.local_ref,
            jackin_core::LocalImageRef::for_image(&first.id)
        );
        assert_eq!(
            first.local_ref.to_string(),
            format!("openshell/sandbox-from:{}", first.id.full())
        );
        assert_eq!(
            stack.model.local_image_id(&first.local_ref),
            Some(first.id.clone())
        );

        let construct = stack
            .engine
            .find(&plan.construct_build().tag, &CancellationToken::new())
            .await?
            .ok_or("the construct tag is missing")?;
        let builds = plan.final_builds(&construct);
        let role = stack
            .engine
            .find(&builds.role.tag, &CancellationToken::new())
            .await?
            .ok_or("the role tag is missing")?;
        let construct_inspect = stack
            .engine
            .inspect(&construct, &CancellationToken::new())
            .await?;
        let role_inspect = stack
            .engine
            .inspect(&role, &CancellationToken::new())
            .await?;
        let derived_inspect = stack
            .engine
            .inspect(&first.id, &CancellationToken::new())
            .await?;
        assert!(
            role_inspect.layers.starts_with(&construct_inspect.layers),
            "role RootFS inherits only its selected construct base"
        );
        assert!(
            derived_inspect.layers.starts_with(&role_inspect.layers),
            "derived RootFS inherits its selected role base"
        );
        assert_eq!(first.tag, builds.derived.tag);
        assert_eq!(
            events,
            [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
                .into_iter()
                .flat_map(started)
                .collect::<Vec<_>>()
        );

        let images = stack.model.images();
        stack.model.retag_local(first.local_ref.clone(), None);
        assert_eq!(stack.model.local_image_id(&first.local_ref), None);
        let (cached, events) = ensure(&stack.engine, &request).await?;
        let cached = cached?;
        assert_eq!(cached.source, ImageSource::Cache);
        assert_eq!(
            (cached.tag, cached.id.clone(), cached.local_ref.clone()),
            (first.tag, first.id.clone(), first.local_ref.clone())
        );
        assert_eq!(
            stack.model.local_image_id(&cached.local_ref),
            Some(cached.id)
        );
        assert_eq!(stack.model.images(), images);
        assert_eq!(
            events,
            [
                ImageEvent::StageCached(BuildStage::Construct),
                ImageEvent::StageCached(BuildStage::Derived)
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn auxiliary_copy_contexts_do_not_contribute_final_from_ancestry() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (_plan, finals, construct_id) = planned(&stack, &request).await?;
        let (tx, _rx) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        let role_id = stack.engine.build(&finals.role, &tx, &cancel).await?;
        let role_inspect = stack.engine.inspect(&role_id, &cancel).await?;
        let construct_inspect = stack.engine.inspect(&construct_id, &cancel).await?;
        assert!(role_inspect.layers.starts_with(&construct_inspect.layers));

        let mut derived = finals.derived.clone();
        let ContextSource::Materialized(files) = &mut derived.context else {
            return Err("the derived Dockerfile was not materialized".into());
        };
        let dockerfile = files
            .iter_mut()
            .find(|file| file.name == "Dockerfile")
            .ok_or("the derived Dockerfile is missing")?;
        let text = String::from_utf8(dockerfile.bytes.clone())?;
        let wrong_base = text.replace("FROM role AS assembled", "FROM construct AS assembled");
        assert_ne!(text, wrong_base, "the final base fixture did not change");
        dockerfile.bytes = wrong_base.into_bytes();

        let derived_id = stack.engine.build(&derived, &tx, &cancel).await?;
        let derived_inspect = stack.engine.inspect(&derived_id, &cancel).await?;
        assert!(
            derived_inspect
                .layers
                .starts_with(&construct_inspect.layers)
        );
        assert!(
            !derived_inspect.layers.starts_with(&role_inspect.layers),
            "an auxiliary role COPY context supplied final FROM ancestry"
        );
        let request = stack
            .model
            .request_for(&derived.tag)
            .ok_or("the external Docker request is missing")?;
        assert_eq!(request.inputs, [role_id, construct_id]);
        assert_eq!(request.context_names, ["role", "construct"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_dockerfile_stdin_is_not_retained_in_external_request_diagnostics() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let marker = "INERT-DOCKER-STDIN-MARKER-7d9a";
        let mut request = stack.fixture.request()?;
        request
            .dockerfile
            .bytes
            .extend_from_slice(format!("# {marker}\n").as_bytes());
        let expected_digest = Sha256::of(&request.dockerfile.bytes);
        let plan = ImagePlan::new(&request)?;
        let (result, events) = ensure(&stack.engine, &request).await?;
        let image = result?;
        let builds = plan.final_builds(
            &stack
                .engine
                .find(&plan.construct_build().tag, &CancellationToken::new())
                .await?
                .ok_or("the construct tag is missing")?,
        );
        let captured = stack
            .model
            .request_for(&builds.role.tag)
            .ok_or("the role request is missing")?;
        assert!(captured.stdin_digest.is_some());
        let state = format!("{:?}", stack.model);
        let request = format!("{captured:?}");
        let result = format!("{image:?} {events:?}");
        for diagnostic in [&state, &request, &result] {
            assert!(
                !diagnostic.contains(marker),
                "Docker stdin marker appeared in a diagnostic: {diagnostic}"
            );
        }
        let inspect = stack
            .engine
            .inspect(&image.id, &CancellationToken::new())
            .await?;
        assert!(!format!("{inspect:?}").contains(marker));
        assert_eq!(captured.stdin_digest, Some(expected_digest));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_changed_construct_id_rekeys_dependent_images_and_rebuild_forces_all() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let mut request = stack.fixture.request()?;
        let (plan, _, construct) = planned(&stack, &request).await?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        let construct_tag = plan.construct_build().tag.clone();
        let construct_inspect = stack
            .engine
            .inspect(&construct, &CancellationToken::new())
            .await?;
        let replacement = ImageId::parse(&tag_id("replacement construct"))?;
        stack.model.seed_image(
            construct_tag.clone(),
            replacement.clone(),
            construct_inspect,
        );
        stack
            .model
            .retag(construct_tag.clone(), Some(replacement.clone()));
        stack
            .engine
            .attest(&construct_tag, &replacement, &CancellationToken::new())
            .await?;

        let (changed, events) = ensure(&stack.engine, &request).await?;
        let changed = changed?;
        assert_eq!(changed.source, ImageSource::Built);
        assert_ne!(changed.tag, warm.tag);
        assert_eq!(
            events,
            [
                ImageEvent::StageCached(BuildStage::Construct),
                ImageEvent::StageStarted(BuildStage::Role),
                ImageEvent::StageFinished(BuildStage::Role),
                ImageEvent::StageStarted(BuildStage::Derived),
                ImageEvent::StageFinished(BuildStage::Derived)
            ]
        );
        assert!(stack.model.images().contains_key(&warm.tag));
        let changed_builds = plan.final_builds(&replacement);
        let role = stack
            .engine
            .find(&changed_builds.role.tag, &CancellationToken::new())
            .await?
            .ok_or("the rebuilt role tag is missing")?;
        let replacement_inspect = stack
            .engine
            .inspect(&replacement, &CancellationToken::new())
            .await?;
        let role_inspect = stack
            .engine
            .inspect(&role, &CancellationToken::new())
            .await?;
        let changed_inspect = stack
            .engine
            .inspect(&changed.id, &CancellationToken::new())
            .await?;
        assert!(role_inspect.layers.starts_with(&replacement_inspect.layers));
        assert!(changed_inspect.layers.starts_with(&role_inspect.layers));

        request.rebuild = Rebuild::Yes;
        let (forced, events) = ensure(&stack.engine, &request).await?;
        assert_eq!(forced?.source, ImageSource::Built);
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
async fn a_failed_role_build_preserves_the_construct_and_retry_succeeds() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        stack.script(
            &builds.role.tag,
            Script {
                outcome: Outcome::Exit(ProcessStatus::Exited(1)),
                ..Script::default()
            },
        );

        let (failed, events) = ensure(&stack.engine, &request).await?;
        assert!(matches!(
            failed,
            Err(ImageError::Engine(EngineError::BuildFailed {
                stage: BuildStage::Role,
                ..
            }))
        ));
        assert_eq!(
            events,
            [
                ImageEvent::StageCached(BuildStage::Construct),
                ImageEvent::StageStarted(BuildStage::Role)
            ]
        );
        assert_eq!(stack.model.image_id(&builds.role.tag), None);
        assert_eq!(stack.model.image_id(&builds.derived.tag), None);
        assert!(stack.model.local_images().is_empty());

        let (retry, events) = ensure(&stack.engine, &request).await?;
        assert_eq!(retry?.source, ImageSource::Built);
        assert_eq!(
            events.first(),
            Some(&ImageEvent::StageCached(BuildStage::Construct))
        );
        assert!(stack.model.image_id(&builds.derived.tag).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_role_dockerfile_returns_before_external_image_state_changes() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let mut request = stack.fixture.request()?;
        request.dockerfile = source("Dockerfile", b"FROM ubuntu\n")?;
        let (result, events) = ensure(&stack.engine, &request).await?;
        assert!(matches!(
            result,
            Err(ImageError::Dockerfile(DockerfileError::FinalStageNotConstruct {
                line: 1,
                found
            })) if found == "ubuntu"
        ));
        assert!(stack.model.images().is_empty());
        assert!(events.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn builder_failure_blocks_cold_build_and_a_warm_cache_does_not_need_a_builder() -> TestResult
{
    guard(async {
        let cold = Stack::new()?;
        let request = cold.fixture.request()?;
        cold.model.set_builder_driver("docker-container");
        let before = cold.model.images();
        let (result, _) = ensure(&cold.engine, &request).await?;
        assert!(matches!(
            result,
            Err(ImageError::Engine(EngineError::BuilderDriver {
                ref driver,
                ..
            })) if driver == "docker-container"
        ));
        assert_eq!(cold.model.images(), before);

        let warm = Stack::new()?;
        let request = warm.fixture.request()?;
        let (first, _) = ensure(&warm.engine, &request).await?;
        let first = first?;
        let before = warm.model.images();
        warm.model.set_builder_driver("docker-container");
        let (cached, _) = ensure(&warm.engine, &request).await?;
        assert_eq!(cached?.source, ImageSource::Cache);
        assert_eq!(warm.model.images(), before);
        assert_eq!(warm.model.image_id(&first.tag), Some(first.id));
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
async fn build_output_is_public_progress_and_role_failure_leaves_no_final_image() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        stack.script(
            &builds.role.tag,
            role_lines(&[
                (OutputStream::Stdout, "\x1b[32mstep one\x1b[0m"),
                (OutputStream::Stderr, "step two"),
                (OutputStream::Stdout, "step three"),
            ]),
        );
        let (result, events) = ensure(&stack.engine, &request).await?;
        result?;
        let role_start = events
            .iter()
            .position(|event| *event == ImageEvent::StageStarted(BuildStage::Role))
            .ok_or("no role start")?;
        let texts: Vec<String> = events[role_start + 1..role_start + 4]
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
            events[role_start + 4],
            ImageEvent::StageFinished(BuildStage::Role)
        );

        let failing = Stack::new()?;
        let request = failing.fixture.request()?;
        let (plan, builds, construct) = planned(&failing, &request).await?;
        attest_construct(&failing, &plan, &construct).await?;
        failing.script(
            &builds.role.tag,
            Script {
                outcome: Outcome::Exit(ProcessStatus::Exited(2)),
                ..role_lines(&[(OutputStream::Stderr, "\x1b[31merror\x1b[0m: boom")])
            },
        );
        let (result, events) = ensure(&failing.engine, &request).await?;
        assert!(matches!(
            result,
            Err(ImageError::Engine(EngineError::BuildFailed {
                stage: BuildStage::Role,
                status: ProcessStatus::Exited(2),
                ..
            }))
        ));
        assert_eq!(failing.model.image_id(&builds.derived.tag), None);
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Role)),
            "{events:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancellation_while_docker_is_held_does_not_publish_the_image() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        let gate = stack.model.hold_build(builds.role.tag.clone());
        let (tx, mut rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let cancellation = cancel.clone();
        let engine = stack.engine.clone();
        let request_for_task = request.clone();
        let task =
            tokio::spawn(
                async move { ensure_image(&engine, &request_for_task, &tx, &cancel).await },
            );
        gate.wait_arrived().await;
        cancellation.cancel();
        let result = task.await?;
        assert!(matches!(result, Err(ImageError::Cancelled { .. })));
        assert_eq!(stack.model.image_id(&builds.role.tag), None);
        assert_eq!(stack.model.image_id(&builds.derived.tag), None);
        while rx.try_recv().is_ok() {}
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancellation_while_output_is_backpressured_keeps_the_old_tag_and_releases_clock_work()
-> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (_plan, builds, _) = planned(&stack, &request).await?;
        let tag = builds.role.tag.clone();
        let (seed_tx, _seed_rx) = mpsc::channel(1);
        let old_id = stack
            .engine
            .build(&builds.role, &seed_tx, &CancellationToken::new())
            .await?;
        assert_eq!(stack.model.image_id(&tag), Some(old_id.clone()));

        let replacement = ImageId::parse(&tag_id("unfinished role image"))?;
        let lines: Vec<_> = (0..70)
            .map(|index| (OutputStream::Stdout, format!("docker-line-{index}")))
            .collect();
        stack.model.script_build(
            tag.clone(),
            DockerBuildReply::default()
                .with_published_id(replacement)
                .with_lines(lines.clone()),
        );
        let pressure = stack.model.observe_backpressure(tag.clone());
        let (progress, _progress_rx) = mpsc::channel(1);
        progress
            .send(ImageEvent::Line {
                stage: BuildStage::Role,
                text: TerminalText::sanitize(b"occupied"),
            })
            .await?;
        let cancel = CancellationToken::new();
        let mut build = Box::pin(stack.engine.build(&builds.role, &progress, &cancel));
        tokio::select! {
            result = &mut build => return Err(format!("the role build completed before output backpressure: {result:?}").into()),
            () = pressure.wait_arrived() => {}
        }
        assert_eq!(stack.model.image_id(&tag), Some(old_id.clone()));
        assert!(stack.clock.is_idle(), "blocked Docker output holds fake-clock work");
        stack.clock.sleep(Duration::from_secs(7)).await;
        assert_eq!(stack.clock.slept(), [Duration::from_secs(7)]);

        cancel.cancel();
        assert!(matches!(build.await, Err(EngineError::Cancelled { .. })));
        assert_eq!(stack.model.image_id(&tag), Some(old_id.clone()));
        assert!(stack.fixture.work_entries().is_empty());

        let control_lines: Vec<_> = (0..70)
            .map(|index| (OutputStream::Stdout, format!("released-line-{index}")))
            .collect();
        stack.model.script_build(
            tag.clone(),
            DockerBuildReply::default().with_lines(control_lines.clone()),
        );
        let (control_tx, mut control_rx) = mpsc::channel(1);
        let control = async {
            let result = stack
                .engine
                .build(&builds.role, &control_tx, &CancellationToken::new())
                .await;
            drop(control_tx);
            result
        };
        let drain = async {
            let mut delivered = Vec::new();
            while let Some(event) = control_rx.recv().await {
                delivered.extend(match event {
                    ImageEvent::Line { text, .. } => Some(text),
                    _ => None,
                });
            }
            delivered
        };
        let (built, delivered) = tokio::join!(control, drain);
        assert_eq!(built?, old_id);
        assert_eq!(
            delivered,
            control_lines
                .iter()
                .map(|(_, text)| TerminalText::sanitize(text.as_bytes()))
                .collect::<Vec<_>>()
        );
        assert_eq!(stack.model.image_id(&tag), Some(old_id));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ensure_rejects_a_successful_build_whose_tag_resolves_to_a_different_id() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        let other = ImageId::parse(&tag_id("externally retagged result"))?;
        stack.script_reply(
            &builds.derived.tag,
            DockerBuildReply::default().with_published_id(other.clone()),
        );
        let (result, _) = ensure(&stack.engine, &request).await?;
        match result {
            Err(ImageError::NotDelivered { tag, built, found }) => {
                assert_eq!(tag, builds.derived.tag);
                assert_ne!(built, other);
                assert_eq!(found, Some(other.clone()));
            }
            other => return Err(format!("expected NotDelivered, got {other:?}").into()),
        }
        assert_eq!(stack.model.image_id(&builds.derived.tag), Some(other));
        assert!(stack.model.local_images().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_conflicting_native_pin_is_refused_without_replacing_its_mapping() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (first, _) = ensure(&stack.engine, &request).await?;
        let first = first?;
        let conflicting = ImageId::parse(&tag_id("conflicting native image pin"))?;
        stack
            .model
            .retag_local(first.local_ref.clone(), Some(conflicting.clone()));

        let (result, _) = ensure(&stack.engine, &request).await?;
        assert!(matches!(
            result,
            Err(ImageError::Engine(EngineError::LocalImageMismatch {
                reference,
                expected,
                found,
            })) if reference == first.local_ref && expected == first.id && found == conflicting
        ));
        assert_eq!(
            stack.model.local_image_id(&first.local_ref),
            Some(conflicting)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn concurrent_publications_for_one_image_resolve_to_the_same_native_pin() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (built, _) = ensure(&stack.engine, &request).await?;
        let built = built?;
        stack.model.retag_local(built.local_ref.clone(), None);
        let cancel = CancellationToken::new();
        let (left, right) = tokio::join!(
            stack.engine.publish_local(&built.id, &cancel),
            stack.engine.publish_local(&built.id, &cancel)
        );
        assert_eq!(left?, built.local_ref);
        assert_eq!(right?, built.local_ref);
        assert_eq!(stack.model.local_image_id(&built.local_ref), Some(built.id));
        Ok(())
    })
    .await
}

async fn source_moved_during_build(
    stack: Stack,
    request: ImageRequest,
    source: ImageTag,
    build_tag: ImageTag,
    replacement: ImageId,
) -> Result<(Result<BuiltImage, ImageError>, Vec<ImageEvent>), Box<dyn Error>> {
    let gate = stack.model.hold_build(build_tag);
    let (tx, mut rx) = mpsc::channel(64);
    let cancel = CancellationToken::new();
    let engine = stack.engine.clone();
    let request_for_task = request.clone();
    let task =
        tokio::spawn(async move { ensure_image(&engine, &request_for_task, &tx, &cancel).await });
    gate.wait_arrived().await;
    stack.model.retag(source, Some(replacement));
    gate.release();
    let result = task.await?;
    Ok((result, drain(&mut rx)))
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

#[tokio::test]
async fn a_source_tag_moved_during_a_dependent_build_is_rejected_by_its_resolved_id() -> TestResult
{
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        let replacement = ImageId::parse(&tag_id("construct moved during role build"))?;
        let (result, events) = source_moved_during_build(
            stack,
            request,
            plan.construct_build().tag.clone(),
            builds.role.tag.clone(),
            replacement.clone(),
        )
        .await?;
        assert_eq!(
            moved(result)?,
            (
                plan.construct_build().tag.clone(),
                construct,
                Some(replacement)
            )
        );
        assert!(events.contains(&ImageEvent::StageStarted(BuildStage::Role)));
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Role)),
            "{events:?}"
        );
        assert!(
            !events.contains(&ImageEvent::StageStarted(BuildStage::Derived)),
            "{events:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_tag_moved_during_the_derived_build_is_rejected_and_the_gateway_tag_is_observed()
-> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        let role = stack
            .engine
            .find(&builds.role.tag, &CancellationToken::new())
            .await?
            .ok_or("the role image is missing")?;
        let replacement = ImageId::parse(&tag_id("role moved during derived build"))?;
        fs::remove_file(attestation_path(
            stack.fixture.work.as_path(),
            &builds.derived.tag,
        ))?;
        let (result, events) = source_moved_during_build(
            stack,
            request,
            builds.role.tag.clone(),
            builds.derived.tag.clone(),
            replacement.clone(),
        )
        .await?;
        assert_eq!(moved(result)?, (builds.role.tag, role, Some(replacement)));
        assert!(events.contains(&ImageEvent::StageStarted(BuildStage::Derived)));
        assert!(
            !events.contains(&ImageEvent::StageFinished(BuildStage::Derived)),
            "{events:?}"
        );
        assert_eq!(warm.source, ImageSource::Built);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unreadable_docker_inspect_fails_closed() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, _, construct) = planned(&stack, &request).await?;
        stack.model.script_inspect(
            construct.clone(),
            jackin_testkit::DockerCommandReply::exited(
                1,
                b"",
                b"Cannot connect to the Docker daemon at unix:///var/run/docker.sock\n",
            ),
        );
        let (result, _) = ensure(&stack.engine, &request).await?;
        assert!(matches!(
            result,
            Err(ImageError::Engine(EngineError::DigestUnreadable { tag, .. }))
                if tag == plan.construct_build().tag
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_pre_cancelled_ensure_has_no_effect_and_a_cancelled_build_is_not_published() -> TestResult
{
    guard(async {
        let stack = Stack::new()?;
        let mut request = stack.fixture.request()?;
        let before = stack.model.images();
        for rebuild in [Rebuild::No, Rebuild::Yes] {
            request.rebuild = rebuild;
            let (tx, mut rx) = mpsc::channel(32);
            let cancel = CancellationToken::new();
            cancel.cancel();
            let result = ensure_image(&stack.engine, &request, &tx, &cancel).await;
            assert!(matches!(result, Err(ImageError::Cancelled { .. })));
            assert_eq!(stack.model.images(), before);
            assert!(drain(&mut rx).is_empty());
        }

        request.rebuild = Rebuild::No;
        let (plan, _, _) = planned(&stack, &request).await?;
        let gate = stack.model.hold_build(plan.construct_build().tag.clone());
        let (tx, _rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let cancellation = cancel.clone();
        let engine = stack.engine.clone();
        let request_for_task = request.clone();
        let task =
            tokio::spawn(
                async move { ensure_image(&engine, &request_for_task, &tx, &cancel).await },
            );
        gate.wait_arrived().await;
        cancellation.cancel();
        assert!(matches!(task.await?, Err(ImageError::Cancelled { .. })));
        assert!(stack.model.image_id(&plan.construct_build().tag).is_some());
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
async fn dropping_ensure_while_docker_is_held_removes_the_staged_context() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, builds, construct) = planned(&stack, &request).await?;
        attest_construct(&stack, &plan, &construct).await?;
        let gate = stack.model.hold_build(builds.role.tag.clone());
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let mut call = Box::pin(ensure_image(&stack.engine, &request, &tx, &cancel));
        tokio::select! {
            result = &mut call => return Err(format!("the call ended before Docker arrived: {result:?}").into()),
            () = gate.wait_arrived() => {}
        }
        let mut during = Vec::new();
        staged_copies(stack.fixture.work.as_path(), &mut during)?;
        assert!(!during.is_empty(), "the external request has no staged source");
        drop(call);
        let mut after = Vec::new();
        staged_copies(stack.fixture.work.as_path(), &mut after)?;
        assert!(after.is_empty(), "staged files remained after dropping: {after:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_process_stop_fault_reaches_the_image_error() -> TestResult {
    guard(async {
        let alive = GroupFault::Alive {
            after: Duration::from_secs(5),
        };
        let rows: Vec<(&str, ProcessReply)> = vec![
            ("cancel", ProcessReply::cancelled()),
            ("cancel-fault", ProcessReply::cancelled_with(alive.clone())),
            ("timeout-fault", ProcessReply::timeout_with(alive.clone())),
        ];
        for (label, reply) in rows {
            let fixture = Fixture::new()?;
            let request = fixture.request()?;
            let runner = Arc::new(ScriptedProcessRunner::new());
            runner.expect_docker(DockerSubcommand::ImageInspect, &[], reply);
            let inner: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&runner);
            let shared: Arc<dyn ProcessRunner> = Arc::new(DockerOnlyProcessRunner::new(inner));
            let engine = BuildxEngine::new(
                shared,
                Arc::new(StdFileSystem::default()),
                fixture.work.clone(),
            );
            let (result, _) = ensure(&engine, &request).await?;
            match (label, result) {
                ("cancel", Err(ImageError::Cancelled { group: None })) => {}
                ("cancel-fault", Err(ImageError::Cancelled { group })) => {
                    assert_eq!(group, Some(alive.clone()));
                }
                (
                    "timeout-fault",
                    Err(ImageError::Engine(EngineError::Process(ProcessError::Timeout {
                        group,
                        ..
                    }))),
                ) => assert_eq!(group, Some(alive.clone())),
                (_, other) => return Err(format!("{label}: got {other:?}").into()),
            }
        }
        Ok(())
    })
    .await
}

fn attestation_path(work: &Path, tag: &ImageTag) -> PathBuf {
    work.join("attested")
        .join(Sha256::of(tag.to_string().as_bytes()).as_str())
}

#[tokio::test]
async fn a_construct_tag_with_a_mismatched_attestation_is_rebuilt_and_recorded() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, _, _) = planned(&stack, &request).await?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        assert_eq!(warm.source, ImageSource::Built);
        let construct_tag = plan.construct_build().tag.clone();
        let actual = stack
            .model
            .image_id(&construct_tag)
            .ok_or("the construct tag is missing")?;
        let planted = ImageId::parse(&tag_id("wrong attestation"))?;
        fs::write(
            attestation_path(stack.fixture.work.as_path(), &construct_tag),
            planted.to_string(),
        )?;

        let gate = stack.model.hold_build(construct_tag.clone());
        let rebuild = ensure(&stack.engine, &request);
        let release = async {
            gate.wait_arrived().await;
            gate.release();
        };
        let (rebuilt, ()) = tokio::join!(rebuild, release);
        let (rebuilt, _) = rebuilt?;
        let rebuilt = rebuilt?;
        assert_eq!(rebuilt.source, ImageSource::Cache);
        assert_eq!(rebuilt.tag, warm.tag);
        assert_eq!(rebuilt.id, warm.id);
        assert_eq!(
            fs::read_to_string(attestation_path(
                stack.fixture.work.as_path(),
                &construct_tag
            ))?,
            actual.to_string()
        );
        assert_eq!(stack.model.image_id(&construct_tag), Some(actual));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_construct_moved_after_attestation_is_rebuilt_from_the_real_external_tag() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (plan, _, construct) = planned(&stack, &request).await?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        assert_eq!(warm.source, ImageSource::Built);

        let replacement = ImageId::parse(&tag_id("poisoned construct tag"))?;
        let inspect = stack
            .engine
            .inspect(&construct, &CancellationToken::new())
            .await?;
        let construct_tag = plan.construct_build().tag.clone();
        stack
            .model
            .seed_image(construct_tag.clone(), replacement.clone(), inspect);
        stack.model.retag(construct_tag.clone(), Some(replacement));
        let gate = stack.model.hold_build(construct_tag.clone());
        let rebuild = ensure(&stack.engine, &request);
        let release = async {
            gate.wait_arrived().await;
            gate.release();
        };
        let (rebuilt, ()) = tokio::join!(rebuild, release);
        let (rebuilt, _) = rebuilt?;
        let rebuilt = rebuilt?;
        assert_eq!(rebuilt.source, ImageSource::Cache);
        assert_eq!(rebuilt.tag, warm.tag);
        assert_eq!(rebuilt.id, warm.id);
        assert_eq!(
            stack.model.image_id(&construct_tag),
            Some(construct.clone())
        );
        assert_eq!(
            fs::read_to_string(attestation_path(
                stack.fixture.work.as_path(),
                &construct_tag
            ))?,
            construct.to_string()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_derived_tag_with_a_mismatched_attestation_is_rebuilt_and_recorded() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        let planted = ImageId::parse(&tag_id("wrong final attestation"))?;
        fs::write(
            attestation_path(stack.fixture.work.as_path(), &warm.tag),
            planted.to_string(),
        )?;

        let (rebuilt, _) = ensure(&stack.engine, &request).await?;
        let rebuilt = rebuilt?;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_eq!(rebuilt.tag, warm.tag);
        assert_eq!(
            fs::read_to_string(attestation_path(stack.fixture.work.as_path(), &warm.tag))?,
            rebuilt.id.to_string()
        );
        let request = stack.fixture.request()?;
        let plan = ImagePlan::new(&request)?;
        let construct = stack
            .engine
            .find(&plan.construct_build().tag, &CancellationToken::new())
            .await?
            .ok_or("the construct tag is missing")?;
        let builds = plan.final_builds(&construct);
        let role = stack
            .engine
            .find(&builds.role.tag, &CancellationToken::new())
            .await?
            .ok_or("the role tag is missing")?;
        let role_inspect = stack
            .engine
            .inspect(&role, &CancellationToken::new())
            .await?;
        let rebuilt_inspect = stack
            .engine
            .inspect(&rebuilt.id, &CancellationToken::new())
            .await?;
        assert!(rebuilt_inspect.layers.starts_with(&role_inspect.layers));
        let (cached, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(cached?.source, ImageSource::Cache);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_derived_tag_moved_after_attestation_is_rebuilt_from_the_real_external_tag() -> TestResult
{
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (warm, _) = ensure(&stack.engine, &request).await?;
        let warm = warm?;
        let replacement = ImageId::parse(&tag_id("poisoned final tag"))?;
        let inspect = stack
            .engine
            .inspect(&warm.id, &CancellationToken::new())
            .await?;
        stack
            .model
            .seed_image(warm.tag.clone(), replacement.clone(), inspect);
        stack.model.retag(warm.tag.clone(), Some(replacement));
        let (rebuilt, _) = ensure(&stack.engine, &request).await?;
        let rebuilt = rebuilt?;
        assert_eq!(rebuilt.source, ImageSource::Built);
        assert_eq!(rebuilt.tag, warm.tag);
        assert_eq!(stack.model.image_id(&rebuilt.tag), Some(rebuilt.id.clone()));
        assert_eq!(
            stack
                .engine
                .attested(&rebuilt.tag, &CancellationToken::new())
                .await?,
            Some(rebuilt.id)
        );
        let (cached, _) = ensure(&stack.engine, &request).await?;
        assert_eq!(cached?.source, ImageSource::Cache);
        Ok(())
    })
    .await
}

type Damage = fn(&Path, &Path, &str) -> std::io::Result<()>;

#[tokio::test]
async fn malformed_construct_attestation_files_are_not_trusted() -> TestResult {
    guard(async {
        let rows: [(&str, Damage); 3] = [
            ("directory", |_, record, _| fs::create_dir(record)),
            ("symlink", |valid, record, _| {
                std::os::unix::fs::symlink(valid, record)
            }),
            ("oversized", |_, record, id| {
                fs::write(record, format!("{id}{}", " ".repeat(4096)))
            }),
        ];
        for (label, damage) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (plan, _, _) = planned(&stack, &request).await?;
            let (warm, _) = ensure(&stack.engine, &request).await?;
            assert_eq!(warm?.source, ImageSource::Built, "{label}");
            let tag = plan.construct_build().tag.clone();
            let id = stack
                .model
                .image_id(&tag)
                .ok_or("construct tag is missing")?;
            let path = attestation_path(stack.fixture.work.as_path(), &tag);
            let copy = path.with_extension("copy");
            fs::copy(&path, &copy)?;
            fs::remove_file(&path)?;
            damage(&copy, &path, &id.to_string())?;

            let gate = stack.model.hold_build(tag.clone());
            let rebuild = ensure(&stack.engine, &request);
            let release = async {
                gate.wait_arrived().await;
                gate.release();
            };
            let (rebuilt, ()) = tokio::join!(rebuild, release);
            let (rebuilt, _) = rebuilt?;
            let rebuilt = rebuilt?;
            assert_eq!(rebuilt.source, ImageSource::Cache, "{label}");
            assert_eq!(
                fs::read_to_string(&path)?,
                stack
                    .model
                    .image_id(&tag)
                    .ok_or("construct tag is missing")?
                    .to_string(),
                "{label}"
            );
            assert!(
                fs::symlink_metadata(&path)?.file_type().is_file(),
                "{label}"
            );
            let (cached, _) = ensure(&stack.engine, &request).await?;
            assert_eq!(cached?.source, ImageSource::Cache, "{label}");
        }
        Ok(())
    })
    .await
}
