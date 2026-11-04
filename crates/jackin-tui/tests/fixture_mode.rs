// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use jackin_console::app::Route;
use jackin_console::fixtures::Scenario;
use jackin_testkit::{ConsoleRig, FixtureArgs, Rig, TestResult, fixture_app, guard};

#[test]
fn console_fixture_args() -> Result<(), Box<dyn std::error::Error>> {
    for scenario in Scenario::ALL {
        let args = FixtureArgs::try_parse_from([
            "console-fixture",
            "--scenario",
            scenario.name(),
            "--motion",
            "paused",
            "--frame",
            "3",
        ])?;
        assert_eq!(args.scenario, scenario);
        assert_eq!(args.common.frame, 3);
        let mut app = fixture_app(&args);
        assert!(!app.world.clock.running, "{}", scenario.name());
        assert!(app.route() != Route::Capsule || scenario == Scenario::CapsuleMulti);
        let _ = &mut app;
    }
    let default = FixtureArgs::try_parse_from(["console-fixture"])?;
    assert_eq!(default.scenario, Scenario::FirstUse);
    for rejected in [
        vec!["console-fixture", "--scenario", "no-such-scenario"],
        vec!["console-fixture", "--scenario", ""],
        vec!["console-fixture", "--motion", "fast"],
        vec!["console-fixture", "--frame", "-1"],
    ] {
        assert!(
            FixtureArgs::try_parse_from(&rejected).is_err(),
            "{rejected:?} was accepted"
        );
    }
    Ok(())
}

#[tokio::test]
async fn idle_console_is_quiet() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        console.skip_intro();
        console.settle().await?;
        assert_eq!(console.console().inflight(), 0);
        for _ in 0..61 {
            console.elapse(1);
            console.tick(5);
            console.settle().await?;
            assert_eq!(console.console().inflight(), 0);
            assert!(
                console.status_line().is_empty(),
                "{}",
                console.status_line()
            );
        }
        let app = console.console().app();
        assert!(app.world.daemons.is_empty(), "no pane exists");
        assert!(app.world.jobs.is_empty(), "no reply is waiting");
        assert!(
            console.status_line().is_empty(),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}
