// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_console::domain::agent::{Agent as ConsoleAgent, Provider as ConsoleProvider};
use jackin_console::domain::doctor::DoctorFailure;
use jackin_console::domain::launch::{LaunchContent, NextStep};
use jackin_console::domain::tuple::InstanceChoice;
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::screens::Go;
use jackin_core::{Config, Program};
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::{CheckId, GatewayConnector};
use jackin_testkit::{
    ConsoleRig, DoctorHost, EngineSocket, FakeGateway, ProcessReply, Rig, ScriptedConnector,
    TestResult, World, connector_over, guard,
};
use jackin_tui::GatewayState;

const FAILING: &str = "▲ gateway failing";
const STALE: &str = "▲ gateway stale";

fn refused() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "127.0.0.1:8080".to_owned(),
        message: "connection refused".to_owned(),
    }
}

fn probes(world: &World) -> usize {
    world
        .runner
        .scripted()
        .seen()
        .iter()
        .filter(|call| call.program == Program::Openshell && call.args == ["--version"])
        .count()
}

fn row_of(screen: &str, id: CheckId) -> Option<&str> {
    screen
        .lines()
        .filter(|line| line.contains(id.label()))
        .find_map(|line| line.split('│').nth(1))
}

fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn doctor_page(
    world: &World,
    host: DoctorHost,
    socket: Option<&EngineSocket>,
) -> Result<ConsoleRig, Box<dyn std::error::Error>> {
    let mut builder = ConsoleRig::builder(world).host(host).size(200, 40);
    if let Some(socket) = socket {
        builder = builder.engine_socket(socket.path().clone());
    }
    let mut console = builder.build().await?;
    console.skip_intro();
    console.keys("D")?;
    Ok(console)
}

#[tokio::test]
async fn healthy_shows_seven_checks() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let socket = EngineSocket::bind()?;
        let mut console = doctor_page(&rig.world, DoctorHost::healthy(), Some(&socket)).await?;
        let screen = console.screen();
        assert_eq!(CheckId::ALL.len(), 7);
        let mut last = 0;
        for id in CheckId::ALL {
            let row = row_of(&screen, id)
                .ok_or_else(|| format!("{} is missing:\n{screen}", id.label()))?;
            assert!(row.contains("ok"), "{row}");
            let at = screen.find(row).unwrap_or(0);
            assert!(at >= last, "{} is out of order", id.label());
            last = at;
        }
        assert!(screen.contains("all checks pass"), "{screen}");
        assert!(
            !screen.contains(FAILING) && !screen.contains(STALE),
            "{screen}"
        );
        console.keys("esc")?;
        let screen = console.screen();
        assert!(!screen.contains("▲"), "{screen}");
        Ok(())
    })
    .await
}

struct Broken {
    name: &'static str,
    id: CheckId,
    word: &'static str,
    chrome_fails: bool,
}

fn break_host(case: &Broken, world: &World, host: &mut DoctorHost) {
    match case.id {
        CheckId::OpenshellBinary => {
            host.openshell = ProcessReply::spawn("openshell is not on PATH");
        }
        CheckId::GatewayRegistered => host.registered = false,
        CheckId::GatewayHealth => world.gateway.set_serving(false),
        CheckId::PinnedVersion => host.gateway_version = Some("9.9.9".to_owned()),
        CheckId::SshBinary | CheckId::GitBinary | CheckId::EngineSocket => {}
    }
}

fn marks(case: &Broken, screen: &str) -> TestResult {
    for id in CheckId::ALL {
        let row = row_of(screen, id)
            .ok_or_else(|| format!("{}: {} is missing:\n{screen}", case.name, id.label()))?;
        let marked = row.contains("fail") || row.contains("warn");
        if id == case.id && !row.contains(case.word) {
            return Err(format!("{}: {row}", case.name).into());
        }
        if id != case.id && marked {
            return Err(format!("{}: {} is marked wrongly: {row}", case.name, id.label()).into());
        }
    }
    Ok(())
}

async fn one_broken(case: &Broken) -> TestResult {
    let rig = Rig::new().await?;
    let mut host = DoctorHost::healthy();
    break_host(case, &rig.world, &mut host);
    let socket = EngineSocket::bind()?;
    let socket = (case.id != CheckId::EngineSocket).then_some(&socket);
    let mut console = doctor_page(&rig.world, host, socket).await?;
    marks(case, &console.screen())?;
    let at = CheckId::ALL
        .iter()
        .position(|id| *id == case.id)
        .unwrap_or(0);
    for _ in 0..at {
        console.keys("down")?;
    }
    let detail = words(&console.screen());
    assert!(
        detail.contains(case.id.fix()),
        "{}: no fix {} on:\n{detail}",
        case.name,
        case.id.fix()
    );
    assert_eq!(
        console.screen().contains(FAILING),
        case.chrome_fails,
        "{}",
        case.name
    );
    Ok(())
}

#[tokio::test]
async fn each_broken_check() -> TestResult {
    guard(async {
        let cases = [
            Broken {
                name: "missing openshell",
                id: CheckId::OpenshellBinary,
                word: "fail",
                chrome_fails: true,
            },
            Broken {
                name: "unregistered gateway",
                id: CheckId::GatewayRegistered,
                word: "fail",
                chrome_fails: true,
            },
            Broken {
                name: "unhealthy gateway",
                id: CheckId::GatewayHealth,
                word: "fail",
                chrome_fails: true,
            },
            Broken {
                name: "wrong version",
                id: CheckId::PinnedVersion,
                word: "fail",
                chrome_fails: true,
            },
            Broken {
                name: "missing engine socket",
                id: CheckId::EngineSocket,
                word: "warn",
                chrome_fails: false,
            },
        ];
        for case in &cases {
            one_broken(case).await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn failed_gateway_retries() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.seed_all(&rig.config).await?;
        let name = rig.world.gateway.name().clone();
        let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&rig.world.gateway);
        let scripted =
            Arc::new(ScriptedConnector::new().gateway(gateway, vec![Some(refused()), None]));
        let gateways: Arc<dyn GatewayConnector> = Arc::<ScriptedConnector>::clone(&scripted);
        let mut console = ConsoleRig::builder(&rig.world)
            .gateway(name.clone(), GatewayState::Failed(refused()))
            .connector(connector_over(&rig.world, gateways))
            .size(200, 40)
            .build()
            .await?;
        console.skip_intro();
        let screen = console.screen();
        assert!(screen.contains("platform-backend"), "{screen}");
        assert!(!screen.contains("1 running"), "{screen}");
        assert!(screen.contains(FAILING), "{screen}");
        assert_eq!(
            scripted.calls(&name),
            0,
            "the first doctor run must not reconnect"
        );
        console.keys("D")?;
        let screen = console.screen();
        let health = row_of(&screen, CheckId::GatewayHealth).unwrap_or_default();
        assert!(
            health.contains("fail") && health.contains("connection refused"),
            "{screen}"
        );
        let version = row_of(&screen, CheckId::PinnedVersion).unwrap_or_default();
        assert!(version.contains("skipped"), "{screen}");
        console.keys("r")?;
        console.settle().await?;
        assert_eq!(scripted.calls(&name), 1);
        let screen = console.screen();
        assert!(screen.contains(FAILING), "{screen}");
        console.keys("r")?;
        console.settle().await?;
        assert_eq!(scripted.calls(&name), 2);
        let screen = console.screen();
        assert!(!screen.contains(FAILING), "{screen}");
        for id in [CheckId::GatewayHealth, CheckId::PinnedVersion] {
            let row = row_of(&screen, id).unwrap_or_default();
            assert!(row.contains("ok"), "{screen}");
        }
        console.keys("esc")?;
        console.elapse(6);
        console.tick(1);
        console.settle().await?;
        let screen = console.screen();
        assert!(screen.contains("1 running"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stale_at_61_seconds() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = ConsoleRig::builder(&rig.world)
            .size(200, 40)
            .build()
            .await?;
        console.skip_intro();
        console.settle().await?;
        assert_eq!(probes(&rig.world), 1);
        console.elapse(59);
        console.tick(1);
        let screen = console.screen();
        assert!(
            !screen.contains(STALE) && !screen.contains(FAILING),
            "{screen}"
        );
        console.settle().await?;
        assert_eq!(probes(&rig.world), 1, "a rerun before 60 s");
        console.elapse(2);
        console.tick(1);
        let screen = console.screen();
        assert!(screen.contains(STALE), "{screen}");
        console.settle().await?;
        assert_eq!(probes(&rig.world), 2);
        let screen = console.screen();
        assert!(
            !screen.contains(STALE) && !screen.contains(FAILING),
            "{screen}"
        );
        console.keys("D")?;
        assert!(
            console.screen().contains("checked 0 s ago"),
            "{}",
            console.screen()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn launch_on_failed_gateway() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let name = rig.world.gateway.name().clone();
        let mut console = ConsoleRig::builder(&rig.world)
            .gateway(name, GatewayState::Failed(refused()))
            .size(200, 40)
            .build()
            .await?;
        rig.world.gateway.set_serving(false);
        let app = console.console().app();
        let workspace = app
            .world
            .workspaces
            .iter()
            .find(|found| found.name == "platform-backend")
            .map(|found| found.id)
            .ok_or("no platform-backend workspace")?;
        app.go(Go::LaunchMany {
            workspace: Some(workspace),
            role: "sentinel".to_owned(),
            instances: vec![InstanceChoice {
                agent: ConsoleAgent::Codex,
                provider: ConsoleProvider::Zai,
                account: Some("work".to_owned()),
                model: "glm-5.3".to_owned(),
            }],
            task: None,
        });
        let doctor = LaunchContent::openshell()
            .words
            .next_step(&NextStep::RunDoctor)
            .ok_or("the doctor step has no words")?;
        for _ in 0..400 {
            console.tick(1);
            console.settle().await?;
            if console.screen().contains(&doctor) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let screen = console.screen();
        assert!(screen.contains("connection refused"), "{screen}");
        let shown = screen
            .lines()
            .filter_map(|line| line.split('│').nth(1))
            .flat_map(str::split_whitespace)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(shown.contains(&doctor), "{doctor}\n{screen}");
        assert!(rig.world.gateway.sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn held_doctor_waits_the_deadline() -> TestResult {
    let world = World::builder().build()?;
    let started = tokio::time::Instant::now();
    let mut console = ConsoleRig::builder(&world)
        .host(DoctorHost {
            openshell: ProcessReply::until_cancelled(Vec::new()),
            ..DoctorHost::healthy()
        })
        .deadline(Duration::from_secs(3))
        .size(200, 40)
        .build()
        .await?;
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(3), "{waited:?}");
    assert!(waited < Duration::from_secs(4), "{waited:?}");
    console.skip_intro();
    console.keys("D")?;
    let screen = console.screen();
    assert!(screen.contains("Running checks…"), "{screen}");
    assert!(!screen.contains("Checks have not run yet"), "{screen}");
    Ok(())
}

#[tokio::test]
async fn no_usable_gateway_is_failing() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        world
            .seed_all(&Config::parse(
                "default_gateway = \"unreached\"\n\n[gateways.unreached]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n",
            )?)
            .await?;
        let mut console = ConsoleRig::builder(&world).size(200, 40).build().await?;
        console.skip_intro();
        let screen = console.screen();
        assert!(screen.contains(FAILING), "{screen}");
        console.keys("D")?;
        let screen = words(&console.screen());
        assert!(screen.contains("no usable gateway"), "{screen}");
        assert!(!screen.contains("all checks pass"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn held_rerun_says_it_could_not_run() -> TestResult {
    let world = World::builder().build()?;
    let mut console = ConsoleRig::builder(&world)
        .host(DoctorHost {
            openshell: ProcessReply::until_cancelled(Vec::new()),
            ..DoctorHost::healthy()
        })
        .deadline(Duration::from_secs(3))
        .size(200, 40)
        .build()
        .await?;
    console.skip_intro();
    console.tick(1);
    console.settle().await?;
    let screen = console.screen();
    assert!(screen.contains(FAILING), "{screen}");
    console.keys("D")?;
    let screen = words(&console.screen());
    let reason = Vocabulary::openshell().doctor_failure(&DoctorFailure::TimedOut);
    assert!(screen.contains(&reason), "{reason}: {screen}");
    Ok(())
}
