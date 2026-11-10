// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_console::domain::instance::InstanceStatus;
use jackin_core::{ConfigEdit, Phase, WorkspaceName};
use jackin_openshell::Gateway;
use jackin_testkit::{TestResult, guard};

use super::support::Lab;

async fn launched()
-> Result<(Lab, jackin_console::domain::instance::InstanceId), Box<dyn std::error::Error>> {
    let rig = jackin_testkit::Rig::new().await?;
    rig.launch(1).await?;
    let mut lab = Lab::with(rig).await?;
    let id = lab
        .world()
        .instances
        .first()
        .ok_or("no instance")?
        .id()
        .clone();
    Ok((lab, id))
}

#[tokio::test]
async fn stop_moves_a_ready_sandbox_to_stopped() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        lab.perform(Effect::Stop { instance: id }).await?;
        assert_eq!(
            lab.world().instances[0]
                .as_available()
                .ok_or("available instance")?
                .status,
            InstanceStatus::RestoreAvailable
        );
        assert_eq!(lab.rig.handle(1).await?.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stop_on_a_completed_sandbox_says_the_main_process_ended() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.rig
            .world
            .gateway
            .force_phase(&name, Phase::Completed { exit_code: 0 });
        lab.perform(Effect::Stop { instance: id }).await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("main process of") && screen.contains("has ended"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_guarded_purge_refuses_unexported_work() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let handle = lab.rig.handle(1).await?;
        handle.commit_in_project("c.txt", b"c", "work").await?;
        lab.perform(Effect::Purge { instance: id }).await?;
        let screen = lab.console.screen();
        assert!(screen.contains("is not exported"), "{screen}");
        assert!(lab.rig.world.gateway.sandbox(&handle.name()).is_some());
        assert_eq!(lab.world().instances.len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn purge_deletes_a_sandbox_with_nothing_to_export() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.perform(Effect::Purge { instance: id }).await?;
        assert!(lab.rig.world.gateway.sandbox(&name).is_none());
        assert!(lab.world().instances.is_empty());
        Ok(())
    })
    .await
}

const WORKSPACE: &str = "platform-backend";

async fn refused_with(lab: &mut Lab, name: &str) -> TestResult {
    let screen = lab.console.screen();
    let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(name) && flat.contains("export and purge"),
        "{screen}"
    );
    let config = lab.config().await?;
    assert!(
        config
            .workspace(&WorkspaceName::parse(WORKSPACE)?)
            .is_some()
    );
    assert!(
        config
            .workspace(&WorkspaceName::parse("renamed")?)
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn deleting_a_workspace_with_a_sandbox_is_refused_and_names_the_sandbox() -> TestResult {
    guard(async {
        let (mut lab, _) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        let id = lab.workspace(WORKSPACE).ok_or("no workspace")?.id;
        lab.perform(Effect::DeleteWorkspace { id }).await?;
        refused_with(&mut lab, name.as_str()).await
    })
    .await
}

#[tokio::test]
async fn renaming_a_workspace_with_a_sandbox_is_refused_and_names_the_sandbox() -> TestResult {
    guard(async {
        let (mut lab, _) = launched().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.console.keys("down e enter")?;
        lab.console.type_text("renamed");
        lab.console.keys("enter")?;
        lab.save().await?;
        refused_with(&mut lab, name.as_str()).await
    })
    .await
}

async fn unavailable_lab(
    phase: Phase,
) -> Result<
    (
        Lab,
        jackin_testkit::SandboxHandle,
        jackin_console::domain::instance::InstanceId,
    ),
    Box<dyn std::error::Error>,
> {
    let rig = jackin_testkit::Rig::new().await?;
    let name = jackin_core::SandboxName::parse("jackin-broken-abcd")?;
    rig.world.gateway.insert_unmanaged(
        &name,
        jackin_core::Labels::from_pairs([
            ("jackin.dev/managed".to_owned(), "true".to_owned()),
            (
                "jackin.dev/workspace".to_owned(),
                "platform-backend".to_owned(),
            ),
            ("jackin.dev/role".to_owned(), "sentinel".to_owned()),
        ])?,
        phase,
    )?;
    let handle = rig.world.gateway.sandbox(&name).ok_or("degraded sandbox")?;
    let mut lab = Lab::with(rig).await?;
    let entry = lab
        .world()
        .instances
        .iter()
        .find(|entry| entry.id().sandbox_name == name.to_string())
        .cloned()
        .ok_or("degraded row")?;
    let jackin_console::domain::instance::InstanceEntry::Unavailable(unavailable) = entry else {
        return Err("degraded row was fabricated as available".into());
    };
    Ok((lab, handle, unavailable.id))
}

#[tokio::test]
async fn degraded_power_stop_and_start_preserve_metadata_without_restarting_agents() -> TestResult {
    guard(async {
        let (mut lab, handle, id) = unavailable_lab(Phase::Ready).await?;
        let reason = match lab
            .world()
            .instances
            .iter()
            .find(|entry| entry.id() == &id)
            .ok_or("degraded row")?
        {
            jackin_console::domain::instance::InstanceEntry::Unavailable(unavailable) => {
                unavailable.reason.clone()
            }
            jackin_console::domain::instance::InstanceEntry::Available(_) => {
                return Err("degraded row became available".into());
            }
        };
        let preflight = jackin_core::sandbox_layout::hook_path(jackin_core::HookKind::Preflight);
        let runs = handle.runs(&preflight).len();
        let marker = handle.file(&jackin_core::sandbox_layout::RUN_START);
        lab.perform(Effect::PowerStop {
            instance: id.clone(),
        })
        .await?;
        assert_eq!(handle.phase(), Phase::Stopped);
        match lab
            .world()
            .instances
            .iter()
            .find(|entry| entry.id() == &id)
            .ok_or("stopped degraded row")?
        {
            jackin_console::domain::instance::InstanceEntry::Unavailable(unavailable) => {
                assert_eq!(
                    unavailable.phase,
                    jackin_console::domain::instance::SandboxPhase::Stopped
                );
                assert_eq!(unavailable.reason, reason);
            }
            jackin_console::domain::instance::InstanceEntry::Available(_) => {
                return Err("stopped degraded row became available".into());
            }
        }
        lab.perform(Effect::PowerStart {
            instance: id.clone(),
        })
        .await?;
        assert_eq!(handle.phase(), Phase::Ready);
        match lab
            .world()
            .instances
            .iter()
            .find(|entry| entry.id() == &id)
            .ok_or("started degraded row")?
        {
            jackin_console::domain::instance::InstanceEntry::Unavailable(unavailable) => {
                assert_eq!(
                    unavailable.phase,
                    jackin_console::domain::instance::SandboxPhase::Ready
                );
                assert_eq!(unavailable.reason, reason);
            }
            jackin_console::domain::instance::InstanceEntry::Available(_) => {
                return Err("started degraded row became available".into());
            }
        }
        assert_eq!(handle.runs(&preflight).len(), runs);
        assert_eq!(handle.file(&jackin_core::sandbox_layout::RUN_START), marker);
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn degraded_power_rejects_invalid_phases_with_an_error() -> TestResult {
    guard(async {
        let (mut ready, ready_handle, ready_id) = unavailable_lab(Phase::Ready).await?;
        ready
            .perform(Effect::PowerStart { instance: ready_id })
            .await?;
        assert!(
            ready
                .console
                .screen()
                .contains("only stopped sandboxes can start")
        );
        assert_eq!(ready_handle.phase(), Phase::Ready);

        let (mut stopped, stopped_handle, stopped_id) = unavailable_lab(Phase::Stopped).await?;
        stopped
            .perform(Effect::PowerStop {
                instance: stopped_id,
            })
            .await?;
        assert!(
            stopped
                .console
                .screen()
                .contains("only ready sandboxes can stop")
        );
        assert_eq!(stopped_handle.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn degraded_power_rejects_available_rows_without_changing_them() -> TestResult {
    guard(async {
        let (mut lab, id) = launched().await?;
        lab.perform(Effect::PowerStop {
            instance: id.clone(),
        })
        .await?;
        assert!(lab.console.screen().contains("not in the roster"));
        assert_eq!(lab.rig.handle(1).await?.phase(), Phase::Ready);

        lab.perform(Effect::Stop {
            instance: id.clone(),
        })
        .await?;
        let handle = lab.rig.handle(1).await?;
        let preflight = jackin_core::sandbox_layout::hook_path(jackin_core::HookKind::Preflight);
        let runs = handle.runs(&preflight).len();
        let marker = handle.file(&jackin_core::sandbox_layout::RUN_START);
        lab.perform(Effect::PowerStart { instance: id }).await?;
        assert!(lab.console.screen().contains("not in the roster"));
        assert_eq!(handle.phase(), Phase::Stopped);
        assert_eq!(handle.runs(&preflight).len(), runs);
        assert_eq!(handle.file(&jackin_core::sandbox_layout::RUN_START), marker);
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn degraded_power_rejects_unmanaged_full_ids() -> TestResult {
    guard(async {
        let rig = jackin_testkit::Rig::new().await?;
        let ready = jackin_core::SandboxName::parse("jackin-ghost-ready")?;
        let stopped = jackin_core::SandboxName::parse("jackin-ghost-stop")?;
        for (name, phase) in [(&ready, Phase::Ready), (&stopped, Phase::Stopped)] {
            rig.world.gateway.insert_unmanaged(
                name,
                jackin_core::Labels::from_pairs([])?,
                phase,
            )?;
        }
        let gateway = rig.world.gateway.name().to_string();
        let mut lab = Lab::with(rig).await?;
        assert!(lab.world().instances.is_empty());
        for (name, effect) in [
            (
                &ready,
                Effect::PowerStop {
                    instance: jackin_console::domain::instance::InstanceId::new(
                        &gateway,
                        ready.to_string(),
                    ),
                },
            ),
            (
                &stopped,
                Effect::PowerStart {
                    instance: jackin_console::domain::instance::InstanceId::new(
                        &gateway,
                        stopped.to_string(),
                    ),
                },
            ),
        ] {
            lab.perform(effect).await?;
            assert!(lab.console.screen().contains("not in the roster"));
            assert_eq!(
                lab.rig
                    .world
                    .gateway
                    .sandbox(name)
                    .ok_or("sandbox")?
                    .phase(),
                if name == &ready {
                    Phase::Ready
                } else {
                    Phase::Stopped
                }
            );
        }
        Ok(())
    })
    .await
}

fn observed(
    gateway: &std::sync::Arc<jackin_testkit::FakeGateway>,
    handle: &jackin_testkit::SandboxHandle,
) -> Result<jackin_tui::ObservedSandbox, Box<dyn std::error::Error>> {
    use jackin_openshell::Gateway as _;

    Ok(jackin_tui::ObservedSandbox {
        gateway: gateway.name().clone(),
        summary: jackin_core::SandboxSummary {
            name: handle.name(),
            labels: jackin_core::SandboxLabels::from_labels(&handle.labels())?,
            phase: handle.phase(),
            created_at: handle.created_at(),
        },
        supervisor: None,
        work: None,
        task: None,
    })
}

async fn projected_ids(
    rig: &jackin_testkit::Rig,
    sandboxes: Vec<jackin_tui::ObservedSandbox>,
) -> Result<Vec<jackin_console::domain::instance::InstanceId>, Box<dyn std::error::Error>> {
    let config = rig.world.config.load().await?;
    let fleet = jackin_tui::Fleet {
        sandboxes,
        unreadable: Vec::new(),
        gateways: Vec::new(),
    };
    let facts = jackin_tui::Facts {
        version: "test".to_owned(),
        config_path: "config.toml".to_owned(),
        home: "/home/test".to_owned(),
        now_secs: 0,
    };
    let snapshot = jackin_tui::project(
        &jackin_tui::ProjectInput {
            config: &config,
            fleet: &fleet,
            facts: &facts,
            roles: &std::collections::BTreeMap::new(),
            checks: &std::collections::BTreeMap::new(),
            origins: &std::collections::BTreeMap::new(),
        },
        &mut jackin_tui::IdTable::new(),
    );
    Ok(snapshot
        .instances
        .iter()
        .map(|entry| entry.id().clone())
        .collect())
}

fn peer(
    rig: &jackin_testkit::Rig,
    name: &str,
) -> Result<std::sync::Arc<jackin_testkit::FakeGateway>, Box<dyn std::error::Error>> {
    let gateway = std::sync::Arc::new(jackin_testkit::FakeGateway::new(
        jackin_core::GatewayName::parse(name)?,
        jackin_core::OpenShellWorkspace::parse("default")?,
    ));
    gateway.link_clock(std::sync::Arc::clone(&rig.world.clock));
    gateway.link_engine(std::sync::Arc::clone(&rig.world.engine));
    Ok(gateway)
}

async fn paired_lab(
    rig: jackin_testkit::Rig,
    second: &std::sync::Arc<jackin_testkit::FakeGateway>,
) -> Result<Lab, Box<dyn std::error::Error>> {
    use jackin_openshell::Gateway as _;

    let entry = rig
        .config
        .gateway(rig.world.gateway.name())
        .cloned()
        .ok_or("home gateway entry")?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddGateway {
            name: second.name().clone(),
            entry,
        }])
        .await?;
    let home: std::sync::Arc<dyn Gateway> =
        std::sync::Arc::<jackin_testkit::FakeGateway>::clone(&rig.world.gateway);
    let other: std::sync::Arc<dyn Gateway> =
        std::sync::Arc::<jackin_testkit::FakeGateway>::clone(second);
    let connector = jackin_testkit::ScriptedConnector::new()
        .gateway(home, Vec::new())
        .gateway(other, Vec::new());
    let mut console = jackin_testkit::ConsoleRig::builder(&rig.world)
        .connector(jackin_testkit::connector_over(
            &rig.world,
            std::sync::Arc::new(connector),
        ))
        .build()
        .await?;
    console.skip_intro();
    Ok(Lab {
        rig,
        console,
        code: std::path::PathBuf::new(),
    })
}

#[tokio::test]
async fn same_sandbox_name_on_two_gateways_routes_by_full_identity() -> TestResult {
    guard(async {
        let rig = jackin_testkit::Rig::new().await?;
        rig.launch(1).await?;
        let home = rig.handle(1).await?;
        let second = peer(&rig, "second-gateway")?;
        second.insert_unmanaged(&home.name(), home.labels(), Phase::Ready)?;
        let other = second.sandbox(&home.name()).ok_or("second sandbox")?;
        let ids = projected_ids(
            &rig,
            vec![
                observed(&rig.world.gateway, &home)?,
                observed(&second, &other)?,
            ],
        )
        .await?;
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(ids[0].sandbox_name, ids[1].sandbox_name);
        let second_id = ids
            .iter()
            .find(|id| id.gateway == second.name().to_string())
            .cloned()
            .ok_or("second full id")?;
        assert_eq!(second_id.sandbox_name, home.name().to_string());
        let mut lab = paired_lab(rig, &second).await?;
        lab.perform(Effect::Stop {
            instance: second_id,
        })
        .await?;
        assert_eq!(home.phase(), Phase::Ready);
        assert_eq!(other.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn same_suffix_on_one_gateway_routes_by_full_sandbox_name() -> TestResult {
    guard(async {
        let rig = jackin_testkit::Rig::new().await?;
        rig.launch(1).await?;
        let home = rig.handle(1).await?;
        let text = home.name().to_string();
        let suffix = &text[text.len() - 4..];
        let other_name = jackin_core::SandboxName::parse(&format!("jackin-other-{suffix}"))?;
        rig.world
            .gateway
            .insert_unmanaged(&other_name, home.labels(), Phase::Ready)?;
        let other = rig.world.gateway.sandbox(&other_name).ok_or("other")?;
        let ids = projected_ids(
            &rig,
            vec![
                observed(&rig.world.gateway, &home)?,
                observed(&rig.world.gateway, &other)?,
            ],
        )
        .await?;
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(ids[0].short_label(), ids[1].short_label());
        let other_id = ids
            .iter()
            .find(|id| id.sandbox_name == other_name.to_string())
            .cloned()
            .ok_or("other full id")?;
        assert_eq!(other_id.gateway, rig.world.gateway.name().to_string());
        let mut lab = Lab::with(rig).await?;
        lab.perform(Effect::Stop { instance: other_id }).await?;
        assert_eq!(home.phase(), Phase::Ready);
        assert_eq!(other.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}
