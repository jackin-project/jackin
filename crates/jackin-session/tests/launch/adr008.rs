// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{
    AccountName, Agent, NameSuffix, Phase, PlannedLabels, PolicyLabel, RoleName, SandboxName,
    SandboxPath, SandboxSlug, SupervisorState, WorkspaceName, sandbox_layout,
};
use jackin_openshell::{
    ClosedPolicy, Gateway, GatewayContractSandboxCreate, MainProcess, SandboxQuery,
};
use jackin_session::{LaunchError, LaunchStep};

use super::support::{
    TestResult, failure, final_image, guard, has_file, launch_id, launch_rig, path, ready, running,
};
use jackin_testkit::{Rig, SandboxHandle};

type Snapshot = (
    BTreeMap<SandboxPath, Vec<u8>>,
    Phase,
    String,
    Option<SupervisorState>,
);

fn snapshot(handle: &SandboxHandle) -> Snapshot {
    (
        handle.files(),
        handle.phase(),
        format!("{:?}", handle.labels()),
        handle.supervisor(),
    )
}

#[tokio::test]
async fn a_new_launch_id_makes_a_new_sandbox_and_leaves_the_live_one_untouched() -> TestResult {
    guard(async {
        for exited in [false, true] {
            let rig = launch_rig().await?;
            ready(rig.launch_observed(1).await?)?;
            let first = rig.handle(1).await?;
            if exited {
                first.exit_codex(0);
            }
            let before = snapshot(&first);
            let second = ready(rig.launch_observed(2).await?)?;
            assert_eq!(second.sandbox, rig.sandbox_name(2)?);
            assert_eq!(rig.gateway().sandboxes().len(), 2);
            assert_eq!(first.name().as_str(), "jackin-platfor-ij4f");
            assert_eq!(second.sandbox.as_str(), "jackin-platfor-uxe1");
            assert_eq!(snapshot(&first), before);
            assert!(!has_file(&first, &sandbox_layout::env_file(&launch_id(2))));
            let other = rig.handle(2).await?;
            assert_eq!(other.supervisor(), Some(running(1)?));
            assert!(has_file(
                &other,
                &sandbox_layout::uploaded_marker(&launch_id(2))
            ));
            assert!(has_file(
                &other,
                &sandbox_layout::setup_done_marker(&launch_id(2))
            ));
            let (first_tag, first_id) = final_image(&rig, 1).await?;
            let (second_tag, second_id) = final_image(&rig, 2).await?;
            assert_eq!(
                (first_tag, first_id.clone()),
                (second_tag.clone(), second_id.clone())
            );
            assert_eq!(
                rig.world.docker.image_id(&second_tag),
                Some(second_id.clone())
            );
            assert_eq!(first.image_id(), Some(first_id.clone()));
            assert_eq!(other.image_id(), Some(second_id));
            let label = |handle: &SandboxHandle| {
                handle
                    .labels()
                    .iter()
                    .find(|(key, _)| key.to_string() == "jackin.dev/image")
                    .map(|(_, value)| value.to_string())
            };
            assert!(label(&first).is_some());
            assert_eq!(label(&first), label(&other));
        }
        Ok(())
    })
    .await
}

async fn foreign(
    rig: &Rig,
    name: SandboxName,
    launch: u8,
) -> Result<SandboxHandle, Box<dyn Error>> {
    let (tag, image_id) = final_image(rig, 9).await?;
    let agents = jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
        Agent::Codex,
        AccountName::parse("work")?,
    )?)?;
    let labels = PlannedLabels {
        workspace: WorkspaceName::parse("platform-backend")?,
        role: RoleName::parse("sentinel")?,
        agents_hash: agents.hash(),
        lead_agent: Agent::Codex,
        policy: PolicyLabel::of("policy"),
        launch: launch_id(launch),
    }
    .complete(&image_id, jackin_core::DockerLabel::Off);
    rig.gateway()
        .create_contract_sandbox(&GatewayContractSandboxCreate::new(
            name.clone(),
            tag,
            labels,
            Vec::new(),
            ClosedPolicy::parse(&rig.loaded.policy().document().render()?)?,
            MainProcess {
                program: path("/bin/sh")?,
                args: vec!["-c".to_owned(), "sleep 3600".to_owned()],
            },
        ))
        .await?;
    rig.gateway()
        .find_sandboxes(
            &SandboxQuery::Name(name.clone()),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await?;
    rig.gateway()
        .sandbox(&name)
        .ok_or_else(|| "the foreign sandbox is missing".into())
}

#[tokio::test]
async fn a_name_collision_or_an_ambiguous_label_is_a_typed_stop_that_changes_nothing() -> TestResult
{
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(9).await?)?;
        let name = rig.sandbox_name(1)?;
        let other = foreign(&rig, name.clone(), 3).await?;
        let before = snapshot(&other);
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert!(
            matches!(&*failed.error, LaunchError::NameCollision { name: found, other: Some(launch) } if *found == name && *launch == launch_id(3)),
            "{:?}",
            failed.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 2);
        assert_eq!(snapshot(&other), before);

        let rig = launch_rig().await?;
        ready(rig.launch_observed(9).await?)?;
        let make = |suffix: &str| -> Result<SandboxName, Box<dyn Error>> {
            Ok(SandboxName::new(&SandboxSlug::parse("dup")?, &NameSuffix::parse(suffix)?))
        };
        let (a, b) = (make("aaaa")?, make("bbbb")?);
        let first = foreign(&rig, a.clone(), 1).await?;
        let second = foreign(&rig, b.clone(), 1).await?;
        let (before_a, before_b) = (snapshot(&first), snapshot(&second));
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert!(
            matches!(&*failed.error, LaunchError::AmbiguousLaunch { launch, sandboxes } if *launch == launch_id(1) && *sandboxes == [a, b]),
            "{:?}",
            failed.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 3);
        assert_eq!((snapshot(&first), snapshot(&second)), (before_a, before_b));
        Ok(())
    })
    .await
}
