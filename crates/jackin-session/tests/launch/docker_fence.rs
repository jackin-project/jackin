// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    CaFingerprint, Config, ConfigEdit, DockerLabel, EngineCert, EngineCertProblem, Phase,
    SandboxLabels, SandboxPath, WorkspaceName, sandbox_layout,
};
use jackin_image::{ImageEngine, Rebuild};
use jackin_openshell::GatewayOp;
use jackin_session::{
    LaunchError, LaunchEvent, LaunchFailure, LaunchOutcome, LaunchPlan, LaunchStep, LaunchWarning,
    plan_launch,
};
use jackin_testkit::{ENGINE_PORT, EngineCertificateSet, Fault, Outcome, Rig, Trigger, launch_id};
use tokio_util::sync::CancellationToken;

use super::docker_support::{Engine, docker_files, docker_rig, rule_keys, split_failure};
use super::support::{
    TestResult, actual_final_builds, failure, final_image, guard, ready, script_construct_rebuild,
    write,
};

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

fn stale(failed: &LaunchFailure) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    match &*failed.error {
        LaunchError::StaleSandbox {
            launch,
            sandbox,
            differences,
        } => {
            assert_eq!(launch, &launch_id(1));
            assert_eq!(Some(sandbox), failed.sandbox.as_ref());
            let shown = failed.error.to_string();
            let mut parts = vec![
                launch.to_string(),
                sandbox.to_string(),
                "new launch".to_owned(),
            ];
            for difference in differences {
                parts.extend([
                    difference.key.to_string(),
                    difference.found.to_string(),
                    difference.wanted.to_string(),
                ]);
            }
            for part in parts {
                assert!(shown.contains(&part), "{part}: {shown}");
            }
            Ok(differences
                .iter()
                .map(|difference| difference.key.to_string())
                .collect())
        }
        other => Err(format!("expected a stale sandbox, saw {other:?}").into()),
    }
}

const DOCKER_KEY: &str = "jackin.dev/docker";
const IMAGE_KEY: &str = "jackin.dev/image";
const POLICY_KEY: &str = "jackin.dev/policy";

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
            assert_eq!(stale(&failed)?, [DOCKER_KEY, IMAGE_KEY, POLICY_KEY]);
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
        let (tag, original) = final_image(&rig, 1).await?;
        let (plan, _, original_construct) = actual_final_builds(&rig, 1).await?;
        let (construct_tag, changed_construct) = script_construct_rebuild(&rig, 1, 'c')?;
        assert_eq!(construct_tag, plan.construct_build().tag);
        let gate = rig
            .world
            .docker
            .hold_build(plan.construct_build().tag.clone());
        let mut rebuilding = Box::pin(retry(&rig, &rig.config, Rebuild::Yes));
        tokio::select! {
            outcome = rebuilding.as_mut() => return Err(format!("rebuild completed before the construct build: {outcome:?}").into()),
            () = gate.wait_arrived() => {}
        }
        gate.release();
        let (_, failed) = split_failure(rebuilding.await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert_eq!(stale(&failed)?, [IMAGE_KEY]);
        assert_eq!(snapshot(&rig).await?, kept);
        assert_eq!(rig.world.docker.image_id(&tag), Some(original.clone()));
        let (plan, builds, construct) = actual_final_builds(&rig, 1).await?;
        assert_eq!(construct, changed_construct);
        assert_ne!(construct, original_construct);
        assert_eq!(
            rig.world
                .engine
                .attested(&plan.construct_build().tag, &CancellationToken::new())
                .await?,
            Some(construct.clone())
        );
        assert_ne!(builds.derived.tag, tag);
        let rebuilt = rig
            .world
            .engine
            .find(&builds.derived.tag, &CancellationToken::new())
            .await?
            .ok_or("the rebuilt final image is missing")?;
        assert_eq!(
            rig.world.docker.image_id(&builds.derived.tag),
            Some(rebuilt.clone())
        );
        let rebuilt_inspect = rig
            .world
            .engine
            .inspect(&rebuilt, &CancellationToken::new())
            .await?;
        let construct_inspect = rig
            .world
            .engine
            .inspect(&construct, &CancellationToken::new())
            .await?;
        assert!(rebuilt_inspect.layers.starts_with(&construct_inspect.layers));
        assert_eq!(rig.gateway().sandboxes().len(), 1);

        let moved = Config::parse(
            &rig.config
                .render()
                .replace(&format!("port = {ENGINE_PORT}"), "port = 2377"),
        )?;
        let (_, failed) = split_failure(retry(&rig, &moved, Rebuild::No).await?)?;
        assert_eq!(stale(&failed)?, [DOCKER_KEY, IMAGE_KEY, POLICY_KEY]);
        assert_eq!(snapshot(&rig).await?, kept);

        let dockerfile = rig.loaded.root().as_path().join("Dockerfile");
        let mut changed = std::fs::read(&dockerfile)?;
        changed.extend_from_slice(b"\n");
        std::fs::write(dockerfile, changed)?;
        let loaded = rig
            .world
            .roles
            .load_path(rig.loaded.root(), &CancellationToken::new())
            .await?;
        for rebuild in [Rebuild::No, Rebuild::Yes] {
            let request = rig.request(1)?.rebuild(rebuild);
            let plan = plan_launch(&rig.config, &loaded, request)?;
            let outcome = rig
                .run_planned(&rig.world.session, plan, CancellationToken::new())
                .await;
            let (_, failed) = split_failure(outcome)?;
            assert_eq!(failed.step, LaunchStep::Sandbox);
            assert_eq!(stale(&failed)?, [IMAGE_KEY]);
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
        assert!(stale(&failed)?.contains(&DOCKER_KEY.to_owned()));
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

const OTHER_CA: &str =
    "-----BEGIN CERTIFICATE-----\nQ0FDQTJDQUEyQ0FDQTJDQUEyQ0FDQTI=\n-----END CERTIFICATE-----\n";

fn certs_dir(rig: &Rig) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(rig.engine_certs()?.as_path().to_path_buf())
}

fn rewrite(rig: &Rig, cert: EngineCert, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(certs_dir(rig)?.join(cert.file_name()), text)?;
    Ok(())
}

#[tokio::test]
async fn a_retry_with_another_ca_on_the_same_port_is_refused_and_changes_nothing() -> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        ready(rig.launch_observed(1).await?)?;
        let kept = snapshot(&rig).await?;
        rewrite(&rig, EngineCert::Ca, OTHER_CA)?;
        let (_, failed) = split_failure(retry(&rig, &rig.config, Rebuild::No).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert_eq!(failed.sandbox, Some(rig.handle(1).await?.name()));
        assert_eq!(stale(&failed)?, [DOCKER_KEY]);
        let shown = failed.error.to_string();
        let LaunchError::StaleSandbox { differences, .. } = &*failed.error else {
            return Err("not a stale sandbox".into());
        };
        for docker in [&differences[0].found, &differences[0].wanted] {
            let text = docker.to_string();
            let ca = text.rsplit('-').next().ok_or("a label without an engine")?;
            assert_eq!(ca.len(), 16, "{text}");
            assert!(shown.contains(ca), "{ca}: {shown}");
        }
        assert_eq!(snapshot(&rig).await?, kept);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_retry_after_rotating_only_the_client_certificate_and_key_continues() -> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        ready(rig.launch_observed(1).await?)?;
        let labels = rig.handle(1).await?.labels();
        let fresh = EngineCertificateSet::generate()?;
        rewrite(&rig, EngineCert::Cert, &fresh.cert)?;
        rewrite(&rig, EngineCert::Key, &fresh.key)?;
        ready(retry(&rig, &rig.config, Rebuild::No).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        assert_eq!(handle.labels(), labels);
        let files = docker_files(&handle);
        for (cert, text) in [
            (EngineCert::Cert, &fresh.cert),
            (EngineCert::Key, &fresh.key),
        ] {
            assert_eq!(
                files.get(&cert.sandbox_path()).map(Vec::as_slice),
                Some(text.as_bytes()),
                "{cert:?}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_retry_with_the_same_ca_saved_with_other_line_endings_continues() -> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        ready(rig.launch_observed(1).await?)?;
        let labels = rig.handle(1).await?.labels();
        let original = std::fs::read_to_string(certs_dir(&rig)?.join("ca.pem"))?;
        let resaved = format!("{}\n\n", original.replace('\n', "\r\n"));
        rewrite(&rig, EngineCert::Ca, &resaved)?;
        ready(retry(&rig, &rig.config, Rebuild::No).await?)?;
        assert_eq!(rig.handle(1).await?.labels(), labels);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_ca_changed_during_the_build_never_reaches_the_sandbox_under_the_old_fingerprint()
-> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        let ca = certs_dir(&rig)?.join("ca.pem");
        let original = std::fs::read(&ca)?;
        let (_, builds, _) = actual_final_builds(&rig, 1).await?;
        let gate = rig.world.docker.hold_build(builds.role.tag.clone());
        let mut launch = Box::pin(rig.launch_observed(1));
        tokio::select! {
            outcome = launch.as_mut() => return Err(format!("launch ended before the Docker build: {outcome:?}").into()),
            () = gate.wait_arrived() => {}
        }
        std::fs::write(&ca, OTHER_CA)?;
        gate.release();
        let outcome = launch.await?;
        let failed = failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Credentials);
        assert!(
            matches!(
                &*failed.error,
                LaunchError::EngineCert {
                    file: EngineCert::Ca,
                    problem: EngineCertProblem::Changed,
                    ..
                }
            ),
            "{:?}",
            failed.error
        );
        assert!(docker_files(&rig.handle(1).await?).is_empty());
        std::fs::write(&ca, &original)?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        let delivered = docker_files(&handle)
            .get(&EngineCert::Ca.sandbox_path())
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
        assert_eq!(delivered, Some(String::from_utf8(original.clone())?));
        let labelled = SandboxLabels::from_labels(&handle.labels())?.docker;
        let DockerLabel::Engine { ca: labelled, .. } = labelled else {
            return Err("the sandbox has no engine label".into());
        };
        assert_eq!(CaFingerprint::of_ca(&original)?, labelled);
        Ok(())
    })
    .await
}
