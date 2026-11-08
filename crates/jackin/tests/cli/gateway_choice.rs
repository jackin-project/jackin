// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin::ExitClass;
use jackin_core::{GatewayName, OpenShellWorkspace, Phase, SandboxName};
use jackin_openshell::Gateway;
use jackin_testkit::{FakeGateway, Rig, TestResult, guard};

use super::support::{Failure, FixedConnector, Harness, failed};

const ENTRY: &str =
    "openshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";

fn config(default: Option<&str>, gateways: &[&str]) -> String {
    let mut text = default
        .map(|name| format!("default_gateway = \"{name}\"\n\n"))
        .unwrap_or_default();
    for name in gateways {
        text.push_str(&format!("[gateways.{name}]\n{ENTRY}\n"));
    }
    text
}

async fn two() -> Result<(Rig, Harness, SandboxName, SandboxName), Failure> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let first = rig.handle(1).await?;
    let second = Arc::new(FakeGateway::new(
        GatewayName::parse("second")?,
        OpenShellWorkspace::parse("default")?,
    ));
    let other = SandboxName::parse("jackin-other-0001")?;
    second.insert_unmanaged(&other, first.labels(), Phase::Ready)?;
    let first_gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&rig.world.gateway);
    let second_gateway: Arc<dyn Gateway> = second;
    let mut harness = Harness::new(&rig.world);
    harness.connector = FixedConnector::empty()
        .with(first_gateway)
        .with(second_gateway);
    let name = first.name();
    Ok((rig, harness, name, other))
}

async fn listed(harness: &Harness, args: &[&str]) -> Result<Vec<SandboxName>, Failure> {
    let ran = harness.run(args).await?;
    ran.ok()?;
    let output = ran.value()?;
    if output["schema"].as_u64() != Some(2) {
        return Err("unexpected listing schema".into());
    }
    let entries = output["sandboxes"]
        .as_array()
        .ok_or("missing sandboxes array")?;
    entries
        .iter()
        .map(|entry| {
            let name = match entry["status"].as_str() {
                Some("readable") => &entry["sandbox"]["name"],
                Some("unreadable") => &entry["name"],
                _ => return Err("missing or unknown sandbox status".into()),
            };
            Ok(SandboxName::parse(
                name.as_str().ok_or("missing sandbox name")?,
            )?)
        })
        .collect()
}

#[tokio::test]
async fn the_default_the_flag_and_a_single_gateway_choose_the_gateway() -> TestResult {
    guard(async {
        let (_rig, harness, first, other) = two().await?;
        harness.write_config(&config(Some("fake-gateway"), &["fake-gateway", "second"]))?;
        assert_eq!(
            listed(&harness, &["ls", "--json"]).await?,
            vec![first.clone()]
        );
        let before = listed(&harness, &["--gateway", "second", "ls", "--json"]).await?;
        assert_eq!(before, vec![other.clone()]);
        let after = listed(&harness, &["ls", "--json", "--gateway", "second"]).await?;
        assert_eq!(after, vec![other.clone()]);

        harness.write_config(&config(None, &["second"]))?;
        assert_eq!(listed(&harness, &["ls", "--json"]).await?, vec![other]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_ambiguous_unknown_or_missing_gateway_fails_naming_the_remedy() -> TestResult {
    guard(async {
        let (_rig, harness, _first, _other) = two().await?;
        harness.write_config(&config(None, &["fake-gateway", "second"]))?;
        let ambiguous = harness.run(&["ls", "--json"]).await?;
        failed(&ambiguous, ExitClass::Failure, &["fake-gateway", "second"])?;
        let unknown = harness.run(&["--gateway", "zzz", "ls", "--json"]).await?;
        failed(&unknown, ExitClass::Failure, &["zzz"])?;

        harness.write_config("")?;
        let none = harness.run(&["ls", "--json"]).await?;
        failed(&none, ExitClass::Failure, &["jackin gateway add"])?;
        Ok(())
    })
    .await
}
