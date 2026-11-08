// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_console::app::Route;
use jackin_core::Config;
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_testkit::{ConsoleRig, Fault, Rig, TestResult, Trigger, World, guard};
use jackin_tui::GatewayState;
use junie_tui::motion::Motion;

fn route(console: &mut ConsoleRig) -> Route {
    console.console().app().route()
}

fn refused() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "127.0.0.1:8080".to_owned(),
        message: "connection refused".to_owned(),
    }
}

#[tokio::test]
async fn running_sandbox_skips_the_intro() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        assert_eq!(route(&mut console), Route::Manager);
        assert!(
            console.screen().contains("1 running"),
            "{}",
            console.screen()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn empty_fleet_plays_and_enter_skips() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        assert_eq!(route(&mut console), Route::Intro);
        console.tick(5);
        console.keys("enter enter")?;
        assert_eq!(route(&mut console), Route::Manager);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn reduced_motion_follows_the_rule_of_the_baseline() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let mut zeros = ConsoleRig::builder(&rig.world)
            .env("JACKIN_NO_MOTION", "00")
            .build()
            .await?;
        assert_eq!(zeros.console().app().shell.motion, Motion::Reduced);
        let mut empty = ConsoleRig::builder(&rig.world)
            .env("JACKIN_NO_MOTION", "")
            .build()
            .await?;
        assert_eq!(empty.console().app().shell.motion, Motion::Full);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn reduced_motion_from_env() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let mut full = ConsoleRig::new(&rig.world).await?;
        let mut reduced = ConsoleRig::builder(&rig.world)
            .env("JACKIN_NO_MOTION", "1")
            .build()
            .await?;
        let mut off = ConsoleRig::builder(&rig.world)
            .env("JACKIN_NO_MOTION", "0")
            .build()
            .await?;
        assert_eq!(full.console().app().shell.motion, Motion::Full);
        assert_eq!(reduced.console().app().shell.motion, Motion::Reduced);
        assert_eq!(off.console().app().shell.motion, Motion::Full);
        for console in [&mut full, &mut reduced] {
            assert_eq!(route(console), Route::Intro);
            console.tick(100);
        }
        assert_eq!(route(&mut reduced), Route::Manager);
        assert_eq!(route(&mut full), Route::Intro);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn failed_list_shows_the_label() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let name = rig.world.gateway.name().clone();
        let mut console = {
            rig.world.seed_all(&rig.config).await?;
            ConsoleRig::builder(&rig.world)
                .gateway(name, GatewayState::Failed(refused()))
                .build()
                .await?
        };
        assert_eq!(route(&mut console), Route::Manager);
        let status = console.status_line();
        assert!(
            status.contains("Could not confirm running instances"),
            "{status}"
        );
        assert!(!status.to_lowercase().contains("docker"), "{status}");
        Ok(())
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn held_list_waits_the_deadline() -> TestResult {
    let world = World::builder().build()?;
    world
        .seed_config(&Config::parse(
            "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n",
        )?)
        .await?;
    world.gateway.inject(Fault::HoldAt {
        trigger: Trigger::Op(GatewayOp::Find),
    });
    let started = tokio::time::Instant::now();
    let mut console = ConsoleRig::builder(&world)
        .deadline(Duration::from_secs(3))
        .build()
        .await?;
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(3), "{waited:?}");
    assert!(waited < Duration::from_secs(4), "{waited:?}");
    assert_eq!(route(&mut console), Route::Manager);
    assert!(
        console
            .status_line()
            .contains("Could not confirm running instances"),
        "{}",
        console.status_line()
    );
    Ok(())
}
