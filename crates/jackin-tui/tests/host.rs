// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Config, GatewayEntry, GatewayName};
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::GatewayConnector;
use jackin_testkit::{
    ConsoleRig, FakeGateway, Rig, ScriptedConnector, TestResult, World, connector_over, guard,
};
use jackin_tui::{ConsoleError, ConsoleExit, SessionConnector, SessionSet, run_console_with};
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(3);

#[tokio::test]
async fn the_loop_runs_on_a_blocking_thread_while_tokio_tasks_keep_running() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let deps = ConsoleRig::builder(&rig.world).deps().await?;
        let started = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let seen = Arc::clone(&started);
        tokio::spawn(async move {
            while !seen.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            sender.send(7_u8)
        });
        let exit = run_console_with(deps, move |_| {
            started.store(true, Ordering::SeqCst);
            receiver
                .recv_timeout(Duration::from_secs(5))
                .map(drop)
                .map_err(io::Error::other)
        })
        .await?;
        assert_eq!(exit, ConsoleExit::Quit);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_loop_gets_the_console_built_from_the_config() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.seed_all(&rig.config).await?;
        let wanted = rig.config.workspaces().count();
        assert!(wanted > 0);
        let deps = ConsoleRig::builder(&rig.world).deps().await?;
        let (sender, receiver) = mpsc::channel();
        run_console_with(deps, move |app| {
            sender
                .send(app.world.workspaces.len())
                .map_err(io::Error::other)
        })
        .await?;
        assert_eq!(receiver.recv()?, wanted);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_terminal_failure_is_a_terminal_error_with_its_message() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let deps = ConsoleRig::builder(&rig.world).deps().await?;
        let outcome = run_console_with(deps, |_| Err(io::Error::other("raw mode refused"))).await;
        match outcome {
            Err(ConsoleError::Terminal(error)) => {
                assert_eq!(error.to_string(), "raw mode refused");
            }
            other => return Err(format!("expected a terminal error, got {other:?}").into()),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_panic_in_the_loop_is_an_error_and_not_a_hang() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let deps = ConsoleRig::builder(&rig.world).deps().await?;
        let outcome = run_console_with(deps, |_| std::panic::resume_unwind(Box::new("boom"))).await;
        assert!(
            matches!(outcome, Err(ConsoleError::Panicked)),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[derive(Debug)]
struct Mixed {
    up: Arc<dyn SessionConnector>,
}

#[async_trait]
impl SessionConnector for Mixed {
    async fn connect(
        &self,
        name: &GatewayName,
        entry: &GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<jackin_session::Session, GatewayError> {
        match name.as_str() {
            "hangs" => std::future::pending().await,
            "refuses" => Err(GatewayError::Unreachable {
                endpoint: name.to_string(),
                message: "connection refused".to_owned(),
            }),
            _ => self.up.connect(name, entry, cancel).await,
        }
    }
}

const THREE_GATEWAYS: &str = "default_gateway = \"fake-gateway\"

[gateways.fake-gateway]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"

[gateways.refuses]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"

[gateways.hangs]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"
";

#[tokio::test(start_paused = true)]
async fn connect_reaches_every_gateway_at_once_and_cuts_a_silent_one_at_the_deadline() -> TestResult
{
    guard(connect_cuts_the_silent_gateway()).await
}

async fn connect_cuts_the_silent_gateway() -> TestResult {
    let world = World::builder().build()?;
    let config = Config::parse(THREE_GATEWAYS)?;
    let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
    let scripted: Arc<dyn GatewayConnector> =
        Arc::new(ScriptedConnector::new().gateway(gateway, Vec::new()));
    let up = connector_over(&world, scripted);
    let connector = Mixed { up };
    let started = tokio::time::Instant::now();
    let default = GatewayName::parse("fake-gateway")?;
    let sessions = SessionSet::connect(
        &config,
        default.clone(),
        &connector,
        DEADLINE,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(started.elapsed(), DEADLINE);
    assert!(sessions.get(&default).is_some());
    assert!(sessions.get(&GatewayName::parse("refuses")?).is_none());
    assert!(sessions.get(&GatewayName::parse("hangs")?).is_none());
    assert_eq!(sessions.default_name(), &default);
    Ok(())
}
