// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use jackin_console::app::Route;
use jackin_console::domain::launch::{LaunchContent, NextStep};
use jackin_core::{ConfigEdit, GatewayName, HookKind, SupervisorState, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_testkit::{
    ConsoleRig, FakeGateway, Fault, ProgramScript, Rig, RoleFixture, TestResult, Trigger, guard,
};

use super::support::{
    Stage, answer, begin, console, dialog, env_file, labels, launch, only, quiet, record_id, stage,
    until_route, until_shows, until_stage, until_untouched,
};

const SANDBOX: &str = "03 Sandbox";

struct ReleaseHolds {
    gateway: Arc<FakeGateway>,
    upload: Trigger,
    recovery: Trigger,
    released: bool,
}

struct ReleaseHold {
    gateway: Arc<FakeGateway>,
    trigger: Trigger,
    released: bool,
}

impl ReleaseHold {
    fn release(&mut self) {
        if !self.released {
            self.gateway.release(&self.trigger);
            self.released = true;
        }
    }
}

impl Drop for ReleaseHold {
    fn drop(&mut self) {
        self.release();
    }
}

impl ReleaseHolds {
    fn release(&mut self) {
        if !self.released {
            self.gateway.release(&self.upload);
            self.gateway.release(&self.recovery);
            self.released = true;
        }
    }
}

impl Drop for ReleaseHolds {
    fn drop(&mut self) {
        self.release();
    }
}

async fn tick_until_stage(console: &mut ConsoleRig, name: &str, wanted: Stage) -> bool {
    for _ in 0..4000 {
        console.tick(1);
        if stage(&console.screen(), name) == Some(wanted) {
            return true;
        }
        tokio::task::yield_now().await;
    }
    false
}

fn words(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test]
async fn provisioning_failure_keeps_the_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        rig.gateway().inject(Fault::ProvisionFails { creates: 10 });
        launch(&mut console).await?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        assert_eq!(stage(&screen, SANDBOX), Some(Stage::Failed), "{screen}");
        assert_eq!(stage(&screen, "04 Credentials"), Some(Stage::Queued));
        let handle = only(&rig)?;
        let name = handle.name().to_string();
        let flat = words(&screen);
        assert!(flat.contains(&format!("Sandbox {name}")), "{screen}");
        assert!(flat.contains("provisioning of"), "{screen}");
        let retry = LaunchContent::openshell()
            .words
            .next_step(&NextStep::Retry {
                sandbox: name,
                launch: labels(&handle)?.planned.launch.to_string(),
            })
            .ok_or("the retry has no words")?;
        assert!(dialog(&screen).contains(&retry), "{retry}\n{screen}");
        console.keys("esc")?;
        console.settle().await?;
        assert_eq!(console.console().app().route(), Route::Manager);
        let world = &console.console().app().world;
        let wanted = record_id(&handle);
        assert!(
            world.instances.iter().any(|record| record.id == wanted),
            "{:?}",
            world.instances.iter().map(|i| &i.id).collect::<Vec<_>>()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_with_no_gateway_selected_fails_before_any_stage_and_says_to_run_the_doctor()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let entry = rig
            .config
            .gateways()
            .next()
            .map(|(_, entry)| entry.clone())
            .ok_or("the rig has no gateway")?;
        rig.world
            .config
            .edit(
                vec![
                    ConfigEdit::AddGateway {
                        name: GatewayName::parse("second-gateway")?,
                        entry,
                    },
                    ConfigEdit::SetDefaultGateway(None),
                ],
                &rig.world.cancel,
            )
            .await?;
        let mut console = ConsoleRig::builder(&rig.world)
            .size(160, 44)
            .unchosen()
            .build()
            .await?;
        console.skip_intro();
        begin(&mut console)?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        assert_eq!(stage(&screen, "01 Image"), Some(Stage::Failed), "{screen}");
        assert_eq!(stage(&screen, SANDBOX), Some(Stage::Queued), "{screen}");
        let doctor = LaunchContent::openshell()
            .words
            .next_step(&NextStep::RunDoctor)
            .ok_or("the doctor step has no words")?;
        assert!(dialog(&screen).contains(&doctor), "{doctor}\n{screen}");
        assert!(!words(&screen).contains("unknown gateway"), "{screen}");
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failing_hook_shows_its_clean_output() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        rig.gateway().script_program(
            &sandbox_layout::hook_path(HookKind::Preflight),
            ProgramScript {
                exit_code: 3,
                stdout: b"\x1b[31mred line\x1b[0m\n\x1b]0;CANARY-TITLE\x07after title\n".to_vec(),
                stderr: b"to stderr\n".to_vec(),
            },
        );
        launch(&mut console).await?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        let flat = words(&screen);
        assert!(flat.contains("Stage Hooks"), "{screen}");
        assert!(
            flat.contains("the preflight hook exited with code 3"),
            "{screen}"
        );
        for shown in ["red line", "after title", "to stderr"] {
            assert!(flat.contains(shown), "{shown}\n{screen}");
        }
        for hidden in ["CANARY-TITLE", "\u{1b}"] {
            assert!(!screen.contains(hidden), "{hidden:?}\n{screen}");
        }
        assert_eq!(only(&rig)?.phase(), jackin_core::Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancel_during_upload_keeps_the_sandbox() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        let rig = Rig::builder().role(role).build().await?;
        let mut console = console(&rig).await?;
        console.settle().await?;
        let upload = Trigger::Op(GatewayOp::UploadTree);
        let start = Trigger::Write(sandbox_layout::RUN_START.clone());
        rig.gateway().inject(Fault::HoldAt {
            trigger: upload.clone(),
        });
        rig.gateway().inject(Fault::HoldAt {
            trigger: start.clone(),
        });
        let mut upload_hold = ReleaseHold {
            gateway: Arc::clone(rig.gateway()),
            trigger: upload.clone(),
            released: false,
        };
        let mut start_hold = ReleaseHold {
            gateway: Arc::clone(rig.gateway()),
            trigger: start.clone(),
            released: false,
        };
        begin(&mut console)?;
        assert!(rig.gateway().until_held(&upload).await);
        let stage_reached = tick_until_stage(&mut console, "05 Upload", Stage::Running).await;
        assert!(stage_reached, "{}", console.screen());
        let screen = console.screen();
        let drain_poll = {
            let mut drain = Box::pin(console.console().drain());
            let mut context = Context::from_waker(Waker::noop());
            drain.as_mut().poll(&mut context)
        };
        upload_hold.release();
        assert!(
            matches!(drain_poll, Poll::Ready(())),
            "ordinary drain was pending"
        );
        assert_eq!(
            stage(&screen, "05 Upload"),
            Some(Stage::Running),
            "{screen}"
        );
        let wanted = record_id(&only(&rig)?);
        console.keys("x right enter")?;
        until_untouched(&mut console, "the manager", |c| {
            c.console().app().route() == Route::Manager
        })
        .await?;
        until_untouched(&mut console, "the cancel record on the roster", |c| {
            c.console()
                .app()
                .world
                .instances
                .iter()
                .any(|record| record.id == wanted)
        })
        .await?;
        let went_on = quiet(&mut console, || rig.gateway().held().contains(&start)).await;
        assert!(!went_on, "the cancelled launch reached the start");
        start_hold.release();
        let handle = only(&rig)?;
        assert_eq!(handle.attachments(), 0);
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        assert!(console.console().app().world.daemons.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_detached_launch_panic_refreshes_the_manager_before_finite_work_settles() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        console.settle().await?;
        assert!(console.console().app().world.instances.is_empty());
        rig.gateway().inject(Fault::PanicAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
        });
        launch(&mut console).await?;
        until_stage(&mut console, "05 Upload", Stage::Running).await?;
        let wanted = record_id(&only(&rig)?);
        console.settle_all().await?;
        console.tick(1);
        console.settle().await?;
        assert!(
            console
                .console()
                .app()
                .world
                .instances
                .iter()
                .any(|record| record.id == wanted),
            "{}",
            console.screen()
        );
        assert!(
            words(&console.screen()).contains("stopped"),
            "{}",
            console.screen()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unconfirmed_cancel_refreshes_the_manager_to_find_the_committed_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        console.settle_all().await?;
        assert!(console.console().app().world.instances.is_empty());
        let upload = Trigger::Op(GatewayOp::UploadTree);
        let recovery = Trigger::Op(GatewayOp::Find);
        rig.gateway().inject(Fault::HoldAt {
            trigger: upload.clone(),
        });
        launch(&mut console).await?;
        assert!(rig.gateway().until_held(&upload).await);
        let handle = only(&rig)?;
        let wanted = record_id(&handle);
        assert!(
            !console
                .console()
                .app()
                .world
                .instances
                .iter()
                .any(|record| record.id == wanted)
        );
        rig.gateway().inject(Fault::HideNextLaunchLookup);
        rig.gateway().inject(Fault::HoldAt {
            trigger: recovery.clone(),
        });
        let mut holds = ReleaseHolds {
            gateway: Arc::clone(rig.gateway()),
            upload: upload.clone(),
            recovery: recovery.clone(),
            released: false,
        };
        console.keys("x right enter")?;
        let recovery_held = rig.gateway().until_held(&recovery).await;
        holds.release();
        assert!(
            recovery_held,
            "the launch recovery lookup never reached its hold"
        );
        console.settle_all().await?;
        assert_eq!(console.console().app().route(), Route::Manager);
        assert!(
            console
                .console()
                .app()
                .world
                .instances
                .iter()
                .any(|record| record.id == wanted),
            "{}",
            console.screen()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancel_before_the_sandbox_leaves_none() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        let provider = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: provider.clone(),
        });
        launch(&mut console).await?;
        assert!(rig.gateway().until_held(&provider).await);
        until_stage(&mut console, "02 OpenShell provider", Stage::Running).await?;
        console.keys("x right enter")?;
        until_route(&mut console, Route::Manager).await?;
        rig.gateway().release(&provider);
        let created = quiet(&mut console, || !rig.gateway().sandboxes().is_empty()).await;
        assert!(!created, "the cancelled launch created a sandbox");
        assert!(console.console().app().world.instances.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn env_answers_reach_the_env_file() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        begin(&mut console)?;
        answer(&mut console, Some("world")).await?;
        until_route(&mut console, Route::Capsule).await?;
        let file = env_file(&only(&rig)?)?;
        assert!(file.contains("export GREETING='world'"), "{file}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn esc_in_the_env_form_creates_nothing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        let builds = rig.world.engine.builds().len();
        begin(&mut console)?;
        until_shows(&mut console, "Values for this launch").await?;
        console.keys("esc")?;
        until_route(&mut console, Route::Manager).await?;
        let created = quiet(&mut console, || !rig.gateway().sandboxes().is_empty()).await;
        assert!(!created, "the form's Esc did not stop the launch");
        assert_eq!(rig.world.engine.builds().len(), builds);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_that_leaves_nothing_returns_to_the_manager() -> TestResult {
    guard(async {
        let rig = Rig::builder().workspace_docker(true).build().await?;
        let mut console = console(&rig).await?;
        begin(&mut console)?;
        until_shows(&mut console, "Run id").await?;
        assert!(rig.gateway().sandboxes().is_empty());
        console.keys("esc")?;
        until_route(&mut console, Route::Manager).await?;
        assert!(console.console().app().world.instances.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_login_fails_with_the_credential_next_step() -> TestResult {
    guard(async {
        let rig = Rig::builder().login(false).build().await?;
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        let fix = LaunchContent::openshell()
            .words
            .next_step(&NextStep::FixCredential)
            .ok_or("the credential step has no words")?;
        assert!(dialog(&screen).contains(&fix), "{fix}\n{screen}");
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}
