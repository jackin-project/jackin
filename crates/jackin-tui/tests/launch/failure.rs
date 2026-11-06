// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::app::Route;
use jackin_console::domain::launch::{LaunchContent, NextStep};
use jackin_core::{ConfigEdit, GatewayName, HookKind, SupervisorState, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_testkit::{ConsoleRig, Fault, ProgramScript, Rig, TestResult, Trigger, guard};

use super::support::{
    Stage, answer, begin, console, dialog, env_file, labels, launch, only, quiet, record_id, stage,
    until_route, until_shows, until_stage, until_untouched,
};

const SANDBOX: &str = "03 Sandbox";

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
            .edit(vec![
                ConfigEdit::AddGateway {
                    name: GatewayName::parse("second-gateway")?,
                    entry,
                },
                ConfigEdit::SetDefaultGateway(None),
            ])
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
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        let upload = Trigger::Op(GatewayOp::UploadTree);
        let start = Trigger::Write(sandbox_layout::RUN_START.clone());
        for trigger in [&upload, &start] {
            rig.gateway().inject(Fault::HoldAt {
                trigger: trigger.clone(),
            });
        }
        launch(&mut console).await?;
        assert!(rig.gateway().until_held(&upload).await);
        until_stage(&mut console, "05 Upload", Stage::Running).await?;
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
        rig.gateway().release(&upload);
        let went_on = quiet(&mut console, || rig.gateway().held().contains(&start)).await;
        assert!(!went_on, "the cancelled launch reached the start");
        let handle = only(&rig)?;
        assert_eq!(handle.attachments(), 0);
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        assert!(console.console().app().world.daemons.is_empty());
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
