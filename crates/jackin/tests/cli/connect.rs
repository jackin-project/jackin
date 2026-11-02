// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin::{ConnectError, ExitClass, GatewayConnector, GrpcConnector};
use jackin_core::{FileSystem, GatewayName, ProcessRunner};
use jackin_testkit::{ProcessReply, ScriptedProgram, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, failed};

const CONFIG: &str = "default_gateway = \"fake-gateway\"

[gateways.fake-gateway]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"
";

const LIST: [&str; 4] = ["gateway", "list", "-o", "json"];

fn connecting(world: &World, reply: ProcessReply) -> Result<(Harness, GrpcConnector), Failure> {
    let mut harness = Harness::new(world);
    harness.write_config(CONFIG)?;
    world
        .runner
        .scripted()
        .expect(ScriptedProgram::Openshell, &LIST, reply);
    let runner: Arc<dyn ProcessRunner> = Arc::<jackin_testkit::TestRunner>::clone(&world.runner);
    let fs: Arc<dyn FileSystem> = Arc::clone(&world.fs);
    let connector = GrpcConnector::new(runner, fs, world.home().clone());
    harness.custom = Some(Arc::new(connector.clone()));
    Ok((harness, connector))
}

#[tokio::test]
async fn a_missing_openshell_is_a_discovery_error_and_not_an_unreachable_gateway() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, connector) = connecting(&world, ProcessReply::spawn("openshell-is-absent"))?;
        let config = harness.stored().await?;
        let name = GatewayName::parse("fake-gateway")?;
        let entry = config.gateway(&name).ok_or("no gateway")?;
        match connector
            .connect(&name, entry, &CancellationToken::new())
            .await
        {
            Err(ConnectError::Discovery { gateway, .. }) => assert_eq!(gateway, name),
            Err(other) => return Err(format!("not a discovery error: {other}").into()),
            Ok(_) => return Err("connected without openshell".into()),
        }
        let ran = harness.run(&["ls"]).await?;
        failed(
            &ran,
            ExitClass::Failure,
            &["fake-gateway", "openshell-is-absent"],
        )?;
        assert!(!ran.stderr.contains("unreachable"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_during_discovery_exits_cancelled_at_once() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, _) = connecting(&world, ProcessReply::until_cancelled(Vec::new()))?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["ls"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        Ok(())
    })
    .await
}
