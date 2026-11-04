// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_console::domain::instance::{ID_PREFIX, InstanceStatus};
use jackin_console::domain::vocabulary::{Unbuilt, Vocabulary};
use jackin_core::{ConfigEdit, Labels, Phase, SandboxName, WorkspaceName};
use jackin_openshell::{GatewayError, GatewayOp, PowerAction};
use jackin_testkit::{ConsoleRig, Fault, Rig, TestResult, World, guard, launch_id};
use jackin_tui::GatewayState;
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
        assert_eq!(first.account.as_deref(), Some("work"));
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
        let mut console = ConsoleRig::builder(&rig.world)
            .gateway(
                jackin_core::GatewayName::parse("lost")?,
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

fn words(what: Unbuilt) -> String {
    Vocabulary::openshell().unbuilt(what)
}

fn failed_text(rig: &mut ConsoleRig, effect: Effect, expected: Unbuilt) {
    let world = &mut rig.console().app().world;
    world.perform(effect);
    let jobs: Vec<String> = world
        .jobs
        .iter()
        .map(|job| format!("{:?}", job.work))
        .collect();
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    assert!(jobs[0].contains(&words(expected)), "{}", jobs[0]);
}

async fn bare() -> Result<(Rig, ConsoleRig), Box<dyn std::error::Error>> {
    let rig = Rig::new().await?;
    let mut console = ConsoleRig::seeded(&rig).await?;
    console.skip_intro();
    Ok((rig, console))
}

#[tokio::test]
async fn unbuilt_workspace_save_fails_visibly() -> TestResult {
    guard(async {
        let (rig, mut console) = bare().await?;
        let before = console.config_text().await?;
        console.keys("down e enter x ctrl+s right enter")?;
        console.settle().await?;
        let screen = console.screen();
        assert!(screen.contains(&words(Unbuilt::SaveWorkspace)), "{screen}");
        assert_eq!(console.config_text().await?, before);
        drop(rig);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_workspace_delete_fails_visibly() -> TestResult {
    guard(async {
        let (rig, mut console) = bare().await?;
        let before = console.config_text().await?;
        console.keys("down d right enter")?;
        console.settle().await?;
        let screen = console.screen();
        assert!(
            screen.contains(&words(Unbuilt::DeleteWorkspace)),
            "{screen}"
        );
        assert_eq!(console.config_text().await?, before);
        drop(rig);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_launch_fails_visibly() -> TestResult {
    guard(async {
        let (rig, mut console) = bare().await?;
        console.keys("down enter enter")?;
        console.tick(3);
        console.settle().await?;
        let screen = console.screen();
        assert!(screen.contains(&words(Unbuilt::Launch)), "{screen}");
        assert!(rig.world.gateway.sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_settings_save_fails_visibly() -> TestResult {
    guard(async {
        let (_rig, mut console) = bare().await?;
        let base = console.console().app().world.global.clone();
        let mut global = base.clone();
        global.coauthor_trailer = true;
        failed_text(
            &mut console,
            Effect::SaveGlobal {
                base: Box::new(base),
                global: Box::new(global),
            },
            Unbuilt::SaveSettings,
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_accounts_save_fails_visibly() -> TestResult {
    guard(async {
        let (_rig, mut console) = bare().await?;
        let base = console.console().app().world.accounts.clone();
        failed_text(
            &mut console,
            Effect::SaveAccounts {
                base: Box::new(base.clone()),
                registry: Box::new(base),
            },
            Unbuilt::SaveAccounts,
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_stop_purge_and_restore_fail_visibly() -> TestResult {
    guard(async {
        let (_rig, mut console) = bare().await?;
        let id = "jk-0000".to_owned();
        failed_text(
            &mut console,
            Effect::Stop {
                instance: id.clone(),
            },
            Unbuilt::StopSandbox,
        );
        let (rig2, mut console2) = bare().await?;
        failed_text(
            &mut console2,
            Effect::Purge {
                instance: id.clone(),
            },
            Unbuilt::PurgeSandbox,
        );
        let (_rig3, mut console3) = bare().await?;
        failed_text(
            &mut console3,
            Effect::Restore { instance: id },
            Unbuilt::RestoreSandbox,
        );
        drop(rig2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_usage_and_validation_fail_visibly() -> TestResult {
    guard(async {
        let (_rig, mut console) = bare().await?;
        failed_text(
            &mut console,
            Effect::RefreshAccount {
                account: "work".to_owned(),
                stagger_ms: 0,
            },
            Unbuilt::RefreshUsage,
        );
        let (_rig2, mut console2) = bare().await?;
        failed_text(
            &mut console2,
            Effect::ValidateAccount {
                account: "work".to_owned(),
            },
            Unbuilt::ValidateAccount,
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unbuilt_role_effects_fail_visibly() -> TestResult {
    guard(async {
        for (effect, expected) in [
            (
                Effect::AddRole {
                    location: "/tmp/role".to_owned(),
                },
                Unbuilt::AddRole,
            ),
            (
                Effect::RemoveRole {
                    role: "sentinel".to_owned(),
                },
                Unbuilt::RemoveRole,
            ),
            (
                Effect::LoadRole {
                    role: "sentinel".to_owned(),
                },
                Unbuilt::LoadRole,
            ),
            (
                Effect::TrustRole {
                    role: "sentinel".to_owned(),
                },
                Unbuilt::TrustRole,
            ),
            (Effect::Prewarm { workspace: 1 }, Unbuilt::PrewarmImage),
        ] {
            let (_rig, mut console) = bare().await?;
            failed_text(&mut console, effect, expected);
        }
        Ok(())
    })
    .await
}
