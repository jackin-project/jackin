// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{Labels, Phase, SandboxLabels, SandboxName, SandboxSummary};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_testkit::{Fault, Rig, SandboxHandle, TestResult, World, guard};

use super::support::{Failure, FixedConnector, GATEWAY_CONFIG, Harness, keys, names};

fn summary(handle: &SandboxHandle) -> Result<SandboxSummary, Failure> {
    Ok(SandboxSummary {
        name: handle.name(),
        labels: SandboxLabels::from_labels(&handle.labels())?,
        phase: handle.phase(),
        created_at: handle.created_at(),
    })
}

async fn mixed() -> Result<(Rig, Harness, Vec<SandboxSummary>, SandboxName), Failure> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    rig.launch(2).await?;
    let errored = rig.sandbox_name(2)?;
    rig.world
        .gateway
        .force_phase(&errored, Phase::Error { exit_code: Some(3) });
    let gateway = &rig.world.gateway;
    gateway.insert_unmanaged(
        &SandboxName::parse("jackin-ghost-0000")?,
        Labels::from_pairs([])?,
        Phase::Ready,
    )?;
    let broken = SandboxName::parse("jackin-broken-0002")?;
    let broken_labels = Labels::from_pairs(
        rig.handle(1)
            .await?
            .labels()
            .iter()
            .filter(|(key, _)| key.to_string() != "jackin.dev/agents")
            .map(|(key, value)| {
                let key = key.to_string();
                let value = if key == "jackin.dev/launch" {
                    "ffffffffffffffffffffffffffffffff".to_owned()
                } else {
                    value.to_string()
                };
                (key, value)
            }),
    )?;
    gateway.insert_unmanaged(&broken, broken_labels, Phase::Ready)?;
    let mut expected = vec![
        summary(&rig.handle(1).await?)?,
        summary(&rig.handle(2).await?)?,
    ];
    expected.sort_by(|left, right| left.name.cmp(&right.name));
    let harness = Harness::new(&rig.world);
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness, expected, broken))
}

#[tokio::test]
async fn ls_json_pins_the_schema_and_names_readable_and_unreadable_managed_sandboxes() -> TestResult
{
    guard(async {
        let (_rig, harness, expected, broken) = mixed().await?;
        let ran = harness.run(&["ls", "--json"]).await?;
        ran.ok()?;
        let value = ran.value()?;
        assert_eq!(keys(&value)?, names(&["schema", "sandboxes"]));
        assert_eq!(value["schema"], 2);
        let rows = value["sandboxes"].as_array().ok_or("no rows")?;
        assert_eq!(rows.len(), expected.len() + 1);
        let mut readable = Vec::new();
        for row in rows {
            match row["status"].as_str() {
                Some("readable") => {
                    assert_eq!(keys(row)?, names(&["status", "sandbox", "agent_metadata"]));
                    assert_eq!(
                        keys(&row["sandbox"])?,
                        names(&["name", "labels", "phase", "created_at"])
                    );
                    readable.push(row["sandbox"]["name"].as_str().ok_or("no name")?);
                }
                Some("unreadable") => {
                    assert_eq!(
                        keys(row)?,
                        names(&[
                            "status",
                            "name",
                            "phase",
                            "created_at",
                            "workspace",
                            "role",
                            "launch",
                            "reason"
                        ])
                    );
                    assert_eq!(row["name"], broken.as_str());
                    assert_eq!(row["phase"], "Ready");
                    assert!(
                        row["reason"]
                            .as_str()
                            .is_some_and(|reason| !reason.is_empty())
                    );
                    assert!(!row.to_string().contains("jackin.dev/"));
                }
                status => return Err(format!("unexpected listing status: {status:?}").into()),
            }
        }
        readable.sort_unstable();
        let mut expected_names: Vec<_> = expected.iter().map(|row| row.name.as_str()).collect();
        expected_names.sort_unstable();
        assert_eq!(readable, expected_names);
        assert!(!ran.stdout.contains("jackin-ghost-0000"));
        assert!(ran.stdout.contains(broken.as_str()));
        assert!(
            ran.stdout
                .contains("the agent hash label is missing or invalid")
        );
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ls_without_json_prints_the_managed_names_and_an_empty_gateway_is_a_literal() -> TestResult
{
    guard(async {
        let (_rig, harness, expected, broken) = mixed().await?;
        let ran = harness.run(&["ls"]).await?;
        ran.ok()?;
        for sandbox in &expected {
            assert!(ran.stdout.contains(sandbox.name.as_str()), "{}", ran.stdout);
        }
        assert!(ran.stdout.contains(broken.as_str()), "{}", ran.stdout);
        assert!(ran.stdout.contains("metadata unavailable"), "{}", ran.stdout);
        assert!(!ran.stdout.contains("jackin-ghost-0000"));
        assert!(!ran.stderr.contains("no sandboxes"), "{}", ran.stderr);

        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(
            "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n",
        )?;
        let empty = harness.run(&["ls", "--json"]).await?;
        assert_eq!(empty.exit, ExitClass::Ok);
        assert_eq!(empty.stdout, "{\"schema\":2,\"sandboxes\":[]}\n");
        assert!(empty.stderr.is_empty());
        let mut offline = Harness::new(&world);
        offline.connector = FixedConnector::empty();
        assert_eq!(offline.run(&["ls", "--json"]).await?.exit, ExitClass::Failure);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_gateway_prints_no_sandboxes_in_human_output_only() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(GATEWAY_CONFIG)?;
        let ran = harness.run(&["ls"]).await?;
        assert_eq!(ran.exit, ExitClass::Ok);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert_eq!(ran.stderr, "no sandboxes\n");

        let broken = SandboxName::parse("jackin-broken-0002")?;
        world.gateway.insert_unmanaged(
            &broken,
            Labels::from_pairs([("jackin.dev/managed".to_owned(), "true".to_owned())])?,
            Phase::Ready,
        )?;
        let ran = harness.run(&["ls"]).await?;
        assert_eq!(ran.exit, ExitClass::Ok);
        assert!(ran.stdout.contains("jackin-broken-0002"), "{}", ran.stdout);
        assert!(
            ran.stdout.contains("metadata unavailable"),
            "{}",
            ran.stdout
        );
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_message_with_control_characters_reaches_the_user_escaped() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(GATEWAY_CONFIG)?;
        world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: GatewayError::Protocol {
                message: "boom\u{1b}[2J\nnext".to_owned(),
            },
        });
        let ran = harness.run(&["ls"]).await?;
        assert_eq!(ran.exit, ExitClass::Failure);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert_eq!(ran.stderr, "error: protocol error: boom\\u{1b}[2J\\nnext\n");
        Ok(())
    })
    .await
}
