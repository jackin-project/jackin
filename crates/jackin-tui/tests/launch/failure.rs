// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::app::Route;
use jackin_core::{HookKind, SupervisorState, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, ProgramScript, Rig, TestResult, Trigger, guard};

use super::support::{
    Stage, answer, begin, console, env_file, fresh, launch, only, quiet, running_console, stage,
    until, until_route, until_shows, until_stage,
};

const SANDBOX: &str = "03 Sandbox";

fn words(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test]
async fn provisioning_failure_keeps_the_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = running_console(&rig).await?;
        rig.gateway().inject(Fault::ProvisionFails { creates: 10 });
        launch(&mut console).await?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        assert_eq!(stage(&screen, SANDBOX), Some(Stage::Failed), "{screen}");
        assert_eq!(stage(&screen, "04 Credentials"), Some(Stage::Queued));
        let handle = fresh(&rig)?;
        let name = handle.name().to_string();
        let flat = words(&screen);
        assert!(flat.contains(&format!("Sandbox {name}")), "{screen}");
        assert!(flat.contains("provisioning of"), "{screen}");
        assert!(flat.contains("retry with"), "{screen}");
        console.keys("esc")?;
        console.settle().await?;
        assert_eq!(console.console().app().route(), Route::Manager);
        let world = &console.console().app().world;
        let wanted = format!("jk-{}", &name[name.len() - 4..]);
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
async fn a_failing_hook_shows_its_clean_output() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = running_console(&rig).await?;
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
        assert_eq!(fresh(&rig)?.phase(), jackin_core::Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancel_during_upload_keeps_the_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = running_console(&rig).await?;
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
        console.keys("x right enter")?;
        until_route(&mut console, Route::Manager).await?;
        until(&mut console, "the roster lists the sandbox", |c| {
            c.console().app().world.instances.len() == 2
        })
        .await?;
        rig.gateway().release(&upload);
        let went_on = quiet(&mut console, || rig.gateway().held().contains(&start)).await;
        assert!(!went_on, "the cancelled launch reached the start");
        let handle = fresh(&rig)?;
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
        until_route(&mut console, Route::Outro).await?;
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
        let mut console = running_console(&rig).await?;
        let builds = rig.world.engine.builds().len();
        begin(&mut console)?;
        until_shows(&mut console, "Values for this launch").await?;
        console.keys("esc")?;
        until_route(&mut console, Route::Manager).await?;
        let created = quiet(&mut console, || rig.gateway().sandboxes().len() > 1).await;
        assert!(!created, "the form's Esc did not stop the launch");
        assert_eq!(rig.world.engine.builds().len(), builds);
        Ok(())
    })
    .await
}
