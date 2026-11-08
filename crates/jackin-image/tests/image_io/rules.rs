// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::error::Error;

use jackin_core::{ImageId, ImageTag};
use jackin_image::{
    BuildStage, BuiltImage, EngineError, ImageEngine, ImageError, ImageInspect, ImagePlan,
    ImageRequest, ImageRule, ImageSource, ensure_image,
};
use jackin_testkit::{DockerBuildReply, DockerCommandReply};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{Stack, TestResult};

type InspectMutation = fn(&ImageInspect) -> ImageInspect;
type FinalRuleCase = (&'static str, InspectMutation, ImageRule);
type StageRuleCase = (&'static str, InspectMutation, (BuildStage, ImageRule));

fn inspect(layers: Vec<String>, env: &[&str], on_build: &[&str]) -> ImageInspect {
    ImageInspect {
        layers,
        env: env.iter().map(|item| (*item).to_owned()).collect(),
        on_build: on_build.iter().map(|item| (*item).to_owned()).collect(),
    }
}

fn on_construct(construct: &ImageInspect, env: &[&str], on_build: &[&str]) -> ImageInspect {
    let mut layers = construct.layers.clone();
    layers.push(format!("sha256:{}", "a".repeat(64)));
    inspect(layers, env, on_build)
}

async fn prepared(
    stack: &Stack,
    request: &ImageRequest,
) -> Result<(ImagePlan, jackin_image::FinalBuilds, ImageInspect), Box<dyn Error>> {
    let plan = ImagePlan::new(request)?;
    let cancel = CancellationToken::new();
    let construct = if let Some(id) = stack
        .engine
        .find(&plan.construct_build().tag, &cancel)
        .await?
    {
        id
    } else {
        let (tx, rx) = mpsc::channel(8);
        drop(rx);
        stack
            .engine
            .build(plan.construct_build(), &tx, &cancel)
            .await?
    };
    stack
        .engine
        .attest(&plan.construct_build().tag, &construct, &cancel)
        .await?;
    let construct_inspect = stack.engine.inspect(&construct, &cancel).await?;
    Ok((
        plan.clone(),
        plan.final_builds(&construct),
        construct_inspect,
    ))
}

async fn ensure(
    stack: &Stack,
    request: &ImageRequest,
) -> Result<Result<BuiltImage, ImageError>, Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel(256);
    let result = ensure_image(&stack.engine, request, &tx, &CancellationToken::new()).await;
    rx.close();
    Ok(result)
}

fn refused(
    result: Result<BuiltImage, ImageError>,
) -> Result<(BuildStage, ImageRule), Box<dyn Error>> {
    match result {
        Err(ImageError::Rule { stage, rule, .. }) => Ok((stage, rule)),
        other => Err(format!("expected an image rule error, got {other:?}").into()),
    }
}

fn client_env(name: &str) -> ImageRule {
    ImageRule::EngineClientEnv {
        name: name.to_owned(),
    }
}

fn client_trigger(trigger: &str) -> ImageRule {
    ImageRule::EngineClientOnBuild {
        trigger: trigger.to_owned(),
    }
}

fn script_inspect(stack: &Stack, tag: &ImageTag, inspect: ImageInspect) {
    stack.model.script_build(
        tag.clone(),
        DockerBuildReply::default().with_inspect(inspect),
    );
}

fn script_bad_inspect_twice(stack: &Stack, tag: &ImageTag, inspect: ImageInspect) {
    script_inspect(stack, tag, inspect.clone());
    script_inspect(stack, tag, inspect);
}

#[tokio::test]
async fn a_role_image_that_does_not_start_with_the_construct_layers_is_refused() -> TestResult {
    guard(async {
        let rows: [(&str, Vec<String>); 3] = [
            ("another base", vec![format!("sha256:{}", "b".repeat(64))]),
            (
                "construct layer is not first",
                vec![
                    format!("sha256:{}", "b".repeat(64)),
                    format!("sha256:{}", "a".repeat(64)),
                ],
            ),
            ("no layers", Vec::new()),
        ];
        for (label, layers) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (_, finals, _) = prepared(&stack, &request).await?;
            script_inspect(&stack, &finals.role.tag, inspect(layers, &[], &[]));
            let result = ensure(&stack, &request).await?;
            assert_eq!(
                refused(result)?,
                (BuildStage::Role, ImageRule::NotBuiltOnConstruct),
                "{label}"
            );
            assert!(
                stack.model.image_id(&finals.derived.tag).is_none(),
                "{label}: no final image was published"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_image_whose_env_names_the_docker_engine_client_is_refused() -> TestResult {
    guard(async {
        let rows: [(&str, &str, &str); 4] = [
            (
                "docker host",
                "DOCKER_HOST=tcp://evil.example:2375",
                "DOCKER_HOST",
            ),
            ("docker config", "DOCKER_CONFIG=/sandbox/c", "DOCKER_CONFIG"),
            ("buildx", "BUILDX_BUILDER=x", "BUILDX_BUILDER"),
            ("a name without a value", "DOCKER_CONTEXT", "DOCKER_CONTEXT"),
        ];
        for (label, entry, name) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (_, finals, construct) = prepared(&stack, &request).await?;
            script_inspect(
                &stack,
                &finals.role.tag,
                on_construct(&construct, &["PATH=/usr/bin", entry], &[]),
            );
            let result = ensure(&stack, &request).await?;
            assert_eq!(
                refused(result)?,
                (BuildStage::Role, client_env(name)),
                "{label}"
            );
            assert!(
                stack.model.image_id(&finals.derived.tag).is_none(),
                "{label}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_image_whose_onbuild_could_set_the_docker_engine_client_is_refused() -> TestResult {
    guard(async {
        let rows: [&str; 4] = [
            "ENV DOCKER_CONFIG=/sandbox/c",
            "ENV D\\=X DOCKER_HOST=tcp://evil.example:2375",
            "ONBUILD ENV BUILDX_BUILDER=x",
            "ENV $NAME=x",
        ];
        for trigger in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (_, finals, construct) = prepared(&stack, &request).await?;
            script_inspect(
                &stack,
                &finals.role.tag,
                on_construct(&construct, &["PATH=/usr/bin"], &["RUN true", trigger]),
            );
            let result = ensure(&stack, &request).await?;
            assert_eq!(
                refused(result)?,
                (BuildStage::Role, client_trigger(trigger)),
                "{trigger}"
            );
            assert!(
                stack.model.image_id(&finals.derived.tag).is_none(),
                "{trigger}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_image_built_on_construct_with_an_ordinary_config_is_delivered() -> TestResult {
    guard(async {
        let stack = Stack::new()?;
        let request = stack.fixture.request()?;
        let (_, finals, construct) = prepared(&stack, &request).await?;
        script_inspect(
            &stack,
            &finals.role.tag,
            on_construct(
                &construct,
                &[
                    "PATH=/usr/bin",
                    "MY_DOCKER_HOST=x",
                    "DOCKERHOST=x",
                    "docker_host=x",
                ],
                &["RUN make", "ENV FOO=1", "COPY . /src"],
            ),
        );
        let image = ensure(&stack, &request).await??;
        assert_eq!(image.source, ImageSource::Built);
        assert_eq!(stack.model.image_id(&image.tag), Some(image.id.clone()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_final_image_that_breaks_a_rule_is_never_delivered_or_trusted_from_cache() -> TestResult {
    guard(async {
        let rows: [FinalRuleCase; 3] = [
            (
                "env",
                |construct: &ImageInspect| {
                    on_construct(construct, &["DOCKER_HOST=tcp://evil.example:2375"], &[])
                },
                client_env("DOCKER_HOST"),
            ),
            (
                "onbuild",
                |construct: &ImageInspect| on_construct(construct, &[], &["ENV DOCKER_CONFIG=/x"]),
                client_trigger("ENV DOCKER_CONFIG=/x"),
            ),
            (
                "layers",
                |_construct: &ImageInspect| inspect(vec![format!("sha256:{}", "b".repeat(64))], &[], &[]),
                ImageRule::NotBuiltOnConstruct,
            ),
        ];
        for (label, craft, rule) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (_, finals, construct) = prepared(&stack, &request).await?;
            let bad = craft(&construct);
            script_bad_inspect_twice(&stack, &finals.derived.tag, bad);
            let first = ensure(&stack, &request).await?;
            assert_eq!(
                refused(first)?,
                (BuildStage::Derived, rule.clone()),
                "{label} first attempt"
            );
            assert!(stack.model.image_id(&finals.derived.tag).is_some(), "{label} external result");
            let second = ensure(&stack, &request).await?;
            assert_eq!(
                refused(second)?,
                (BuildStage::Derived, rule),
                "{label} retry"
            );
            assert!(
                stack.model.image_id(&finals.derived.tag).is_some(),
                "{label} keeps the untrusted external image visible but never returns it as attested"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_buildx_engine_reads_the_config_and_layers_that_docker_prints() -> TestResult {
    guard(async {
        let rows: [StageRuleCase; 3] = [
            (
                "env",
                |construct: &ImageInspect| {
                    on_construct(construct, &["PATH=/usr/bin", "DOCKER_HOST=tcp://x:1"], &[])
                },
                (BuildStage::Role, client_env("DOCKER_HOST")),
            ),
            (
                "onbuild",
                |construct: &ImageInspect| on_construct(construct, &[], &["ENV DOCKER_CONFIG=/x"]),
                (BuildStage::Role, client_trigger("ENV DOCKER_CONFIG=/x")),
            ),
            (
                "layers",
                |_construct: &ImageInspect| inspect(vec!["sha256:other".to_owned()], &[], &[]),
                (BuildStage::Role, ImageRule::NotBuiltOnConstruct),
            ),
        ];
        for (label, craft, want) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (_, finals, construct) = prepared(&stack, &request).await?;
            script_inspect(&stack, &finals.role.tag, craft(&construct));
            assert_eq!(refused(ensure(&stack, &request).await?)?, want, "{label}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_inspect_the_engine_cannot_read_stops_the_build_without_a_rule_verdict() -> TestResult {
    guard(async {
        let rows = [
            (
                "exit status",
                DockerCommandReply::exited(1, b"", b"inspect failed"),
            ),
            ("not json", DockerCommandReply::exited(0, b"no json\n", b"")),
            (
                "no root fs",
                DockerCommandReply::exited(0, b"{\"Config\":{}}\n", b""),
            ),
        ];
        for (label, reply) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            let (_, finals, construct) = prepared(&stack, &request).await?;
            let id = ImageId::parse(&format!("sha256:{}", "c".repeat(64)))?;
            stack.model.script_build(
                finals.role.tag.clone(),
                DockerBuildReply::default()
                    .with_published_id(id.clone())
                    .with_iid_file(jackin_testkit::DockerIidFile::Text(
                        format!("{id}\n").into_bytes(),
                    ))
                    .with_inspect(on_construct(&construct, &[], &[])),
            );
            stack.model.script_inspect(id, reply);
            let result = ensure(&stack, &request).await?;
            assert!(
                matches!(
                    result,
                    Err(ImageError::Engine(EngineError::InspectUnreadable { .. }))
                ),
                "{label}: {result:?}"
            );
            assert!(
                stack.model.image_id(&finals.derived.tag).is_none(),
                "{label}"
            );
        }
        Ok(())
    })
    .await
}
