// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{Labels, Phase, SandboxName, WorkspaceName};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{FixedConnector, Harness, failed};

const WORKSPACE: &str = "platform-backend";

#[tokio::test]
async fn workspace_rm_is_refused_while_a_sandbox_carries_its_label_and_works_after_the_purge()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let before = harness.config_bytes()?;

        let refused = harness.run(&["workspace", "rm", WORKSPACE]).await?;
        failed(
            &refused,
            ExitClass::Failure,
            &[name.as_str(), WORKSPACE, "export", "purge"],
        )?;
        assert_eq!(harness.config_bytes()?, before);

        harness
            .run(&["purge", name.as_str(), "--force"])
            .await?
            .ok()?;
        harness.run(&["workspace", "rm", WORKSPACE]).await?.ok()?;
        assert!(
            harness
                .stored()
                .await?
                .workspace(&WorkspaceName::parse(WORKSPACE)?)
                .is_none()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn workspace_rm_is_refused_when_a_gateway_cannot_be_asked_about_its_sandboxes() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.connector = FixedConnector::empty();
        harness.write_config(&rig.config.render())?;
        let before = harness.config_bytes()?;

        let refused = harness.run(&["workspace", "rm", WORKSPACE]).await?;
        failed(&refused, ExitClass::Failure, &[])?;
        assert_eq!(harness.config_bytes()?, before);
        Ok(())
    })
    .await
}

fn broken(rig: &Rig, name: &str, workspace: Option<&str>) -> TestResult {
    let mut pairs = vec![("jackin.dev/managed".to_owned(), "true".to_owned())];
    if let Some(workspace) = workspace {
        pairs.push(("jackin.dev/workspace".to_owned(), workspace.to_owned()));
    }
    rig.world.gateway.insert_unmanaged(
        &SandboxName::parse(name)?,
        Labels::from_pairs(pairs)?,
        Phase::Ready,
    )?;
    Ok(())
}

async fn refused_with(name: &str, workspace: Option<&str>, expected: &[&str]) -> TestResult {
    let rig = Rig::new().await?;
    broken(&rig, name, workspace)?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    let before = harness.config_bytes()?;

    let refused = harness.run(&["workspace", "rm", WORKSPACE]).await?;
    failed(&refused, ExitClass::Failure, expected)?;
    assert_eq!(harness.config_bytes()?, before);
    Ok(())
}

#[tokio::test]
async fn workspace_rm_is_refused_while_a_sandbox_whose_labels_name_it_cannot_be_read() -> TestResult
{
    guard(refused_with(
        "jackin-orphan-0001",
        Some(WORKSPACE),
        &[
            "jackin-orphan-0001",
            "fake-gateway",
            "cannot be exported",
            "jackin purge jackin-orphan-0001 --force",
        ],
    ))
    .await
}

#[tokio::test]
async fn workspace_rm_is_refused_while_a_sandbox_without_a_workspace_label_cannot_be_attributed()
-> TestResult {
    guard(refused_with(
        "jackin-nolabel-0002",
        None,
        &[
            "jackin-nolabel-0002",
            "fake-gateway",
            "cannot tell which workspace",
            "cannot be exported",
            "jackin purge jackin-nolabel-0002 --force",
        ],
    ))
    .await
}

#[tokio::test]
async fn workspace_rm_reports_every_kind_of_blocking_sandbox_in_one_refusal() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let launched = rig.sandbox_name(1)?;
        broken(&rig, "jackin-orphan-0001", Some(WORKSPACE))?;
        broken(&rig, "jackin-nolabel-0002", None)?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let before = harness.config_bytes()?;

        let refused = harness.run(&["workspace", "rm", WORKSPACE]).await?;
        failed(
            &refused,
            ExitClass::Failure,
            &[
                &format!("jackin export {launched}"),
                &format!("jackin purge {launched}"),
                "jackin purge jackin-orphan-0001 --force",
                "jackin purge jackin-nolabel-0002 --force",
            ],
        )?;
        assert_eq!(harness.config_bytes()?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_refusal_names_the_gateway_option_when_the_gateway_is_not_the_default() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        broken(&rig, "jackin-orphan-0001", Some(WORKSPACE))?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(
            &rig.config
                .render()
                .replace("default_gateway = \"fake-gateway\"\n", ""),
        )?;

        let refused = harness
            .run(&["--gateway", "fake-gateway", "workspace", "rm", WORKSPACE])
            .await?;
        failed(
            &refused,
            ExitClass::Failure,
            &["jackin purge jackin-orphan-0001 --gateway fake-gateway --force"],
        )?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn workspace_rm_ignores_a_sandbox_with_unreadable_labels_that_names_another_workspace()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        broken(&rig, "jackin-other-0003", Some("elsewhere"))?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;

        harness.run(&["workspace", "rm", WORKSPACE]).await?.ok()?;
        assert!(
            harness
                .stored()
                .await?
                .workspace(&WorkspaceName::parse(WORKSPACE)?)
                .is_none()
        );
        Ok(())
    })
    .await
}
