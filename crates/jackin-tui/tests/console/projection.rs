// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::instance::{ID_PREFIX, InstanceStatus};
use std::collections::BTreeMap;

use jackin_core::{
    AttemptId, ConfigEdit, Labels, Phase, SandboxName, SupervisorReading, SupervisorState,
    TerminalText, WorkspaceName,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, PowerAction};
use jackin_session::{Unexported, WorkState};
use jackin_testkit::{ConsoleRig, Fault, Rig, TestResult, World, guard, launch_id};
use jackin_tui::{Facts, Fleet, GatewayState, IdTable, ObservedSandbox, ProjectInput, project};
use tokio_util::sync::CancellationToken;

fn status_of(console: &mut ConsoleRig, id: u8) -> Option<InstanceStatus> {
    let run = launch_id(id).to_string();
    console
        .console()
        .app()
        .world
        .instances
        .iter()
        .find(|i| i.run_id == run)
        .map(|i| i.status)
}

#[tokio::test]
async fn roster_maps_phase_and_exit_code() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        for id in [1, 2, 3, 4] {
            rig.launch(id).await?;
        }
        rig.handle(2).await?.exit_codex(3);
        rig.handle(3).await?.exit_codex(0);
        rig.world
            .session
            .power(
                &rig.sandbox_name(4)?,
                PowerAction::Stop,
                &CancellationToken::new(),
            )
            .await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        assert_eq!(status_of(&mut console, 1), Some(InstanceStatus::Running));
        assert_eq!(status_of(&mut console, 2), Some(InstanceStatus::Crashed));
        assert_eq!(
            status_of(&mut console, 3),
            Some(InstanceStatus::CleanExited)
        );
        assert_eq!(
            status_of(&mut console, 4),
            Some(InstanceStatus::RestoreAvailable)
        );
        let screen = console.screen();
        assert!(screen.contains("1 running"), "{screen}");
        console.keys("down")?;
        let screen = console.screen();
        for word in ["running", "crashed", "exited", "restore available"] {
            assert!(screen.contains(word), "{word} is not on:\n{screen}");
        }
        let first = console.console().app().world.instances[0].clone();
        assert_eq!(first.role, "sentinel");
        assert!(first.id.starts_with(ID_PREFIX), "{}", first.id);
        assert_eq!(first.account.as_deref(), None);
        assert_eq!(first.workdir, "/sandbox/project");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn roster_hides_unmanaged_and_notes_unreadable() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let gateway = &rig.world.gateway;
        gateway.insert_unmanaged(
            &SandboxName::parse("jackin-ghost-0000")?,
            Labels::from_pairs([])?,
            Phase::Ready,
        )?;
        gateway.insert_unmanaged(
            &SandboxName::parse("jackin-broken-0002")?,
            Labels::from_pairs([("jackin.dev/managed".to_owned(), "true".to_owned())])?,
            Phase::Ready,
        )?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        let instances = &console.console().app().world.instances;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].status, InstanceStatus::Running);
        assert!(
            console
                .status_line()
                .contains("1 sandboxes have labels jackin cannot read"),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn removed_workspace_label_lists_without_workspace() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![ConfigEdit::RemoveWorkspace(WorkspaceName::parse(
                "platform-backend",
            )?)])
            .await?;
        let mut console = ConsoleRig::new(&rig.world).await?;
        let world = &console.console().app().world;
        assert!(world.workspaces.is_empty());
        assert_eq!(world.instances.len(), 1);
        assert_eq!(world.instances[0].workspace, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn list_failure_on_one_gateway() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.seed_all(&rig.config).await?;
        let lost = jackin_core::GatewayName::parse("lost")?;
        let entry = rig
            .config
            .gateways()
            .next()
            .map(|(_, entry)| entry.clone())
            .ok_or("the rig has no gateway")?;
        rig.world
            .config
            .edit(vec![ConfigEdit::AddGateway {
                name: lost.clone(),
                entry,
            }])
            .await?;
        let mut console = ConsoleRig::builder(&rig.world)
            .gateway(
                lost,
                GatewayState::Failed(GatewayError::Unreachable {
                    endpoint: "10.0.0.9:8080".to_owned(),
                    message: "connection refused".to_owned(),
                }),
            )
            .build()
            .await?;
        assert_eq!(status_of(&mut console, 1), Some(InstanceStatus::Running));
        let status = console.status_line();
        assert!(
            status.contains("Could not list sandboxes on lost"),
            "{status}"
        );
        let world = &console.console().app().world;
        assert_eq!(
            world.daemon_health,
            jackin_console::world::DaemonHealth::Stale
        );
        assert_eq!(world.arbiter.discovery, Ok(1));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn list_failure_on_every_gateway() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: GatewayError::Unreachable {
                endpoint: "10.0.0.9:8080".to_owned(),
                message: "connection refused".to_owned(),
            },
        });
        let mut console = ConsoleRig::new(&rig.world).await?;
        assert_eq!(
            console.console().app().route(),
            jackin_console::app::Route::Manager
        );
        assert!(console.console().app().world.instances.is_empty());
        assert_eq!(
            console.console().app().world.arbiter.discovery,
            Err(jackin_console::arbiter::DiscoveryError::IndexUnreadable)
        );
        assert!(
            console
                .status_line()
                .contains("Could not confirm running instances"),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn empty_config_and_fleet() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let mut console = ConsoleRig::new(&world).await?;
        console.skip_intro();
        let screen = console.screen();
        assert!(
            screen.contains("Create a workspace from this directory"),
            "{screen}"
        );
        let world = &console.console().app().world;
        assert!(world.workspaces.is_empty() && world.accounts.accounts.is_empty());
        assert_eq!(
            world.daemon_health,
            jackin_console::world::DaemonHealth::Healthy
        );
        assert!(world.instances.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn refresh_brings_in_a_new_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        console.skip_intro();
        assert!(console.console().app().world.instances.is_empty());
        rig.launch(1).await?;
        for _ in 0..8 {
            console.elapse(1);
            console.tick(1);
        }
        console.settle().await?;
        assert_eq!(status_of(&mut console, 1), Some(InstanceStatus::Running));
        Ok(())
    })
    .await
}

fn left(unusable: &[&str]) -> WorkState {
    WorkState::Unexported(Unexported {
        uncommitted: 0,
        branches: Vec::new(),
        detached: None,
        worktrees: Vec::new(),
        unusable: unusable
            .iter()
            .map(|name| TerminalText::sanitize(name.as_bytes()))
            .collect(),
    })
}

#[tokio::test]
async fn work_that_cannot_be_exported_keeps_the_sandbox_marked_as_preserved() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let summary = rig
            .world
            .session
            .list()
            .await?
            .sandboxes
            .into_iter()
            .next()
            .ok_or("no sandbox")?;
        let config = rig.world.config.load().await?;
        let facts = Facts {
            version: "test".to_owned(),
            config_path: "config.toml".to_owned(),
            home: "/home/test".to_owned(),
            now_secs: 0,
        };
        let exited = SupervisorReading::Reported(SupervisorState::Exited {
            attempt: AttemptId::FIRST,
            code: 0,
        });
        let cases = [
            (
                Some(left(&["worktree at /sandbox/x"])),
                InstanceStatus::PreservedUnpushed,
            ),
            (Some(left(&[])), InstanceStatus::CleanExited),
            (Some(WorkState::Exported), InstanceStatus::CleanExited),
            (None, InstanceStatus::CleanExited),
        ];
        for (work, expected) in cases {
            let fleet = Fleet {
                sandboxes: vec![ObservedSandbox {
                    gateway: rig.world.gateway.name().clone(),
                    summary: summary.clone(),
                    supervisor: Some(exited.clone()),
                    work: work.clone(),
                }],
                unreadable: 0,
                gateways: Vec::new(),
            };
            let snapshot = project(
                &ProjectInput {
                    config: &config,
                    fleet: &fleet,
                    facts: &facts,
                    roles: &BTreeMap::new(),
                    checks: &BTreeMap::new(),
                    origins: &BTreeMap::new(),
                },
                &mut IdTable::new(),
            );
            let statuses: Vec<_> = snapshot.instances.iter().map(|i| i.status).collect();
            assert_eq!(statuses, vec![expected], "{work:?}");
        }
        Ok(())
    })
    .await
}
