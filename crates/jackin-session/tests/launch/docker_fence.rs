// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    Config, ConfigEdit, DockerLabel, ImageId, ImageLabel, Phase, SandboxPath, WorkspaceName,
    sandbox_layout,
};
use jackin_image::Rebuild;
use jackin_openshell::GatewayOp;
use jackin_session::{
    LaunchError, LaunchEvent, LaunchFailure, LaunchOutcome, LaunchPlan, LaunchStep, LaunchWarning,
    plan_launch,
};
use jackin_testkit::{ENGINE_PORT, Fault, Outcome, Rig, Trigger, launch_id};
use tokio_util::sync::CancellationToken;

use super::docker_support::{Engine, docker_rig, rule_keys, split_failure};
use super::support::{TestResult, guard, ready, write};

type Snapshot = (
    BTreeMap<SandboxPath, Vec<u8>>,
    Vec<(SandboxPath, Option<jackin_core::FileMode>)>,
    String,
    Vec<String>,
);

async fn snapshot(rig: &Rig) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let handle = rig.handle(1).await?;
    let modes = handle
        .files()
        .keys()
        .map(|path| (path.clone(), handle.file_mode(path)))
        .collect();
    Ok((
        handle.files(),
        modes,
        format!("{:?}", handle.labels()),
        rule_keys(rig, &handle).await?,
    ))
}

async fn retry(
    rig: &Rig,
    config: &Config,
    rebuild: Rebuild,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    let request = rig.request(1)?.rebuild(rebuild);
    let plan: LaunchPlan = plan_launch(config, &rig.loaded, request)?;
    Ok(rig
        .run_planned(&rig.world.session, plan, CancellationToken::new())
        .await)
}

fn stale(
    failed: &LaunchFailure,
) -> Result<(ImageLabel, ImageLabel, DockerLabel, DockerLabel), Box<dyn std::error::Error>> {
    match &*failed.error {
        LaunchError::StaleSandbox {
            launch,
            sandbox,
            found_image,
            wanted_image,
            found_docker,
            wanted_docker,
        } => {
            assert_eq!(launch, &launch_id(1));
            assert_eq!(Some(sandbox), failed.sandbox.as_ref());
            let shown = failed.error.to_string();
            for part in [
                launch.to_string(),
                sandbox.to_string(),
                found_image.to_string(),
                wanted_image.to_string(),
                "new launch".to_owned(),
            ] {
                assert!(shown.contains(&part), "{part}: {shown}");
            }
            Ok((
                found_image.clone(),
                wanted_image.clone(),
                *found_docker,
                *wanted_docker,
            ))
        }
        other => Err(format!("expected a stale sandbox, saw {other:?}").into()),
    }
}

fn flipped(rig: &Rig, docker: bool) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = rig.config.clone();
    config.apply(ConfigEdit::SetWorkspaceDocker {
        workspace: WorkspaceName::parse("platform-backend")?,
        docker,
    })?;
    Ok(config)
}

#[tokio::test]
async fn a_retry_whose_capability_differs_from_the_sandbox_is_refused_and_changes_nothing()
-> TestResult {
    guard(async {
        for (before, after) in [(true, false), (false, true)] {
            let rig = docker_rig(false, before, Engine::WithCerts).await?;
            ready(rig.launch_observed(1).await?)?;
            let env = sandbox_layout::env_file(&launch_id(1));
            write(&rig, &rig.handle(1).await?.name(), &env, "SENTINEL=1\n").await?;
            let kept = snapshot(&rig).await?;
            let outcome = retry(&rig, &flipped(&rig, after)?, Rebuild::No).await?;
            let (events, failed) = split_failure(outcome)?;
            assert_eq!(failed.step, LaunchStep::Sandbox);
            assert_eq!(failed.sandbox, Some(rig.handle(1).await?.name()));
            let (found, wanted, found_docker, wanted_docker) = stale(&failed)?;
            assert_ne!(found, wanted);
            assert_ne!(found_docker, wanted_docker);
            assert_eq!(snapshot(&rig).await?, kept);
            assert_eq!(rig.gateway().sandboxes().len(), 1);
            assert_eq!(
                events.first() == Some(&LaunchEvent::Warning(LaunchWarning::DockerCapability)),
                after
            );
            if after {
                assert_eq!(
                    events.get(1),
                    Some(&LaunchEvent::StepStarted(LaunchStep::Image))
                );
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_retry_with_another_image_a_rebuild_that_changes_the_image_or_another_engine_port_is_refused()
-> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        ready(rig.launch_observed(1).await?)?;
        let kept = snapshot(&rig).await?;
        let same = retry(&rig, &rig.config, Rebuild::Yes).await?;
        let builds = rig.world.engine.builds().len();
        ready(same)?;
        assert!(builds > 3, "the rebuild did not run: {builds}");
        assert_eq!(rig.gateway().sandboxes().len(), 1);

        let moved = Config::parse(
            &rig.config
                .render()
                .replace(&format!("port = {ENGINE_PORT}"), "port = 2377"),
        )?;
        let (_, failed) = split_failure(retry(&rig, &moved, Rebuild::No).await?)?;
        let (found, wanted, found_docker, wanted_docker) = stale(&failed)?;
        assert_eq!(found, wanted);
        assert_ne!(found_docker, wanted_docker);
        assert_eq!(snapshot(&rig).await?, kept);

        let tag = rig.final_image()?.tag;
        let other = ImageId::parse(&format!("sha256:{}", "9".repeat(64)))?;
        rig.world.engine.pin_id(&tag, other);
        for rebuild in [Rebuild::No, Rebuild::Yes] {
            let (_, failed) = split_failure(retry(&rig, &rig.config, rebuild).await?)?;
            assert_eq!(failed.step, LaunchStep::Sandbox);
            let (found, wanted, found_docker, wanted_docker) = stale(&failed)?;
            assert_ne!(found, wanted);
            assert_eq!(found_docker, wanted_docker);
            assert_eq!(snapshot(&rig).await?, kept);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_flipped_retry_is_refused_while_the_sandbox_is_still_provisioning() -> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::ProvisionFor { polls: 400 });
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Create),
            token: token.clone(),
        });
        let first = rig.run(rig.request(1)?, token).await?;
        assert!(matches!(first.result, Ok(LaunchOutcome::Cancelled(_))));
        let handle = rig.handle(1).await?;
        assert_ne!(handle.phase(), Phase::Ready);
        let outcome = retry(&rig, &flipped(&rig, false)?, Rebuild::No).await?;
        let (_, failed) = split_failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        let (_, _, found_docker, wanted_docker) = stale(&failed)?;
        assert_ne!(found_docker, wanted_docker);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}
