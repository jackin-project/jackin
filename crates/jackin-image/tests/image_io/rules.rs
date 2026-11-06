// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::error::Error;

use jackin_image::{
    BuildStage, BuiltImage, EngineError, ImageEngine, ImageError, ImageInspect, ImageRequest,
    ImageRule, ImageSource, ensure_image,
};
use jackin_testkit::{FakeEngine, construct_layer};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{Fixture, Reply, Stack, TestResult, model_construct_layer};

fn inspect(layers: &[&str], env: &[&str], on_build: &[&str]) -> ImageInspect {
    let text = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
    ImageInspect {
        layers: text(layers),
        env: text(env),
        on_build: text(on_build),
    }
}

fn on_construct(env: &[&str], on_build: &[&str]) -> ImageInspect {
    let base = construct_layer();
    let own = format!("sha256:{}", "a".repeat(64));
    inspect(&[base.as_str(), own.as_str()], env, on_build)
}

async fn ensure(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
) -> Result<Result<BuiltImage, ImageError>, Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel(256);
    let result = ensure_image(engine, request, &tx, &CancellationToken::new()).await;
    rx.close();
    Ok(result)
}

fn built_stages(engine: &FakeEngine) -> Vec<BuildStage> {
    engine.builds().iter().map(|record| record.stage).collect()
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

#[tokio::test]
async fn a_role_image_that_does_not_start_with_the_construct_layers_is_refused_before_the_derived_build()
-> TestResult {
    guard(async {
        let base = construct_layer();
        let ubuntu = format!("sha256:{}", "b".repeat(64));
        let rows = [
            ("another base", inspect(&[ubuntu.as_str()], &[], &[])),
            (
                "the construct layer is not first",
                inspect(&[ubuntu.as_str(), base.as_str()], &[], &[]),
            ),
            ("no layers", inspect(&[], &[], &[])),
        ];
        for (label, crafted) in rows {
            let request = Fixture::new()?.request()?;
            let engine = FakeEngine::new();
            engine.craft_inspect(BuildStage::Role, crafted);
            let result = ensure(&engine, &request).await?;
            assert_eq!(
                refused(result)?,
                (BuildStage::Role, ImageRule::NotBuiltOnConstruct),
                "{label}"
            );
            assert_eq!(
                built_stages(&engine),
                [BuildStage::Construct, BuildStage::Role],
                "{label}: the derived build must not start"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_image_whose_env_names_the_docker_engine_client_is_refused() -> TestResult {
    guard(async {
        let rows = [
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
            let request = Fixture::new()?.request()?;
            let engine = FakeEngine::new();
            engine.craft_inspect(
                BuildStage::Role,
                on_construct(&["PATH=/usr/bin", entry], &[]),
            );
            let result = ensure(&engine, &request).await?;
            assert_eq!(
                refused(result)?,
                (BuildStage::Role, client_env(name)),
                "{label}"
            );
            assert_eq!(built_stages(&engine).len(), 2, "{label}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_image_whose_onbuild_could_set_the_docker_engine_client_is_refused() -> TestResult {
    guard(async {
        let rows = [
            "ENV DOCKER_CONFIG=/sandbox/c",
            "ENV D\\=X DOCKER_HOST=tcp://evil.example:2375",
            "ONBUILD ENV BUILDX_BUILDER=x",
            "ENV $NAME=x",
        ];
        for trigger in rows {
            let request = Fixture::new()?.request()?;
            let engine = FakeEngine::new();
            engine.craft_inspect(
                BuildStage::Role,
                on_construct(&["PATH=/usr/bin"], &["RUN true", trigger]),
            );
            let result = ensure(&engine, &request).await?;
            assert_eq!(
                refused(result)?,
                (BuildStage::Role, client_trigger(trigger)),
                "{trigger}"
            );
            assert_eq!(built_stages(&engine).len(), 2, "{trigger}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_image_built_on_construct_with_an_ordinary_config_is_delivered() -> TestResult {
    guard(async {
        let request = Fixture::new()?.request()?;
        let engine = FakeEngine::new();
        engine.craft_inspect(
            BuildStage::Role,
            on_construct(
                &[
                    "PATH=/usr/bin",
                    "MY_DOCKER_HOST=x",
                    "DOCKERHOST=x",
                    "docker_host=x",
                ],
                &["RUN make", "ENV FOO=1", "COPY . /src"],
            ),
        );
        let image = ensure(&engine, &request).await?;
        assert_eq!(image?.source, ImageSource::Built);
        assert_eq!(
            built_stages(&engine),
            [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_final_image_that_breaks_a_rule_is_never_delivered_and_is_not_served_from_cache()
-> TestResult {
    guard(async {
        let base = construct_layer();
        let ubuntu = format!("sha256:{}", "b".repeat(64));
        let rows = [
            (
                "env",
                on_construct(&["DOCKER_HOST=tcp://evil.example:2375"], &[]),
                ImageRule::EngineClientEnv {
                    name: "DOCKER_HOST".to_owned(),
                },
            ),
            (
                "onbuild",
                on_construct(&[], &["ENV DOCKER_CONFIG=/x"]),
                client_trigger("ENV DOCKER_CONFIG=/x"),
            ),
            (
                "layers",
                inspect(&[ubuntu.as_str(), base.as_str()], &[], &[]),
                ImageRule::NotBuiltOnConstruct,
            ),
        ];
        for (label, crafted, rule) in rows {
            let request = Fixture::new()?.request()?;
            let engine = FakeEngine::new();
            engine.craft_inspect(BuildStage::Derived, crafted);
            let first = ensure(&engine, &request).await?;
            assert_eq!(
                refused(first)?,
                (BuildStage::Derived, rule.clone()),
                "{label}"
            );
            let builds = built_stages(&engine).len();
            let second = ensure(&engine, &request).await?;
            assert_eq!(
                refused(second)?,
                (BuildStage::Derived, rule),
                "{label}: cache"
            );
            assert_eq!(
                built_stages(&engine).len(),
                builds + 2,
                "{label}: an image that failed the scan is rebuilt, never trusted"
            );
        }
        Ok(())
    })
    .await
}

fn docker_json(layers: &[&str], env: &str, on_build: &str) -> String {
    let layers: Vec<String> = layers.iter().map(|layer| format!("\"{layer}\"")).collect();
    format!(
        "{{\"Id\":\"x\",\"Config\":{{\"Env\":{env},\"OnBuild\":{on_build}}},\"RootFS\":{{\"Type\":\"layers\",\"Layers\":[{}]}}}}\n",
        layers.join(",")
    )
}

#[tokio::test]
async fn the_buildx_engine_reads_the_config_and_layers_that_docker_prints() -> TestResult {
    guard(async {
        let layers = [
            model_construct_layer(),
            format!("sha256:{}", "c".repeat(64)),
        ];
        let layers = [layers[0].as_str(), layers[1].as_str()];
        let evil = docker_json(
            &layers,
            "[\"PATH=/usr/bin\",\"DOCKER_HOST=tcp://x:1\"]",
            "null",
        );
        let trigger = docker_json(&layers, "null", "[\"ENV DOCKER_CONFIG=/x\"]");
        let other = docker_json(&["sha256:other"], "null", "null");
        let rows = [
            ("env", evil, (BuildStage::Role, client_env("DOCKER_HOST"))),
            (
                "onbuild",
                trigger,
                (BuildStage::Role, client_trigger("ENV DOCKER_CONFIG=/x")),
            ),
            (
                "layers",
                other,
                (BuildStage::Role, ImageRule::NotBuiltOnConstruct),
            ),
        ];
        for (label, stdout, want) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            stack
                .model
                .inspect_reply(BuildStage::Role, Reply { status: 0, stdout });
            let (tx, mut rx) = mpsc::channel(256);
            let result =
                ensure_image(&stack.engine, &request, &tx, &CancellationToken::new()).await;
            rx.close();
            assert_eq!(refused(result)?, want, "{label}");
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
                Reply {
                    status: 1,
                    stdout: String::new(),
                },
            ),
            (
                "not json",
                Reply {
                    status: 0,
                    stdout: "no json\n".to_owned(),
                },
            ),
            (
                "no root fs",
                Reply {
                    status: 0,
                    stdout: "{\"Config\":{}}\n".to_owned(),
                },
            ),
        ];
        for (label, reply) in rows {
            let stack = Stack::new()?;
            let request = stack.fixture.request()?;
            stack.model.inspect_reply(BuildStage::Role, reply);
            let (tx, mut rx) = mpsc::channel(256);
            let result =
                ensure_image(&stack.engine, &request, &tx, &CancellationToken::new()).await;
            rx.close();
            assert!(
                matches!(
                    result,
                    Err(ImageError::Engine(EngineError::InspectUnreadable { .. }))
                ),
                "{label}: {result:?}"
            );
        }
        Ok(())
    })
    .await
}
