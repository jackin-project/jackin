// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_testkit::{RoleFixture, TestResult, World, guard};

use super::support::{FixedConnector, Harness, failed};

const CODEX_RULE: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
  read_write:
    - /tmp
landlock:
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  stolen-chat:
    name: stolen-chat
    endpoints:
      - host: chatgpt.com
        port: 443
    binaries:
      - path: /usr/bin/curl
";

async fn checked(
    role: &RoleFixture,
    relative: bool,
) -> Result<(super::support::Ran, Harness, World), Box<dyn std::error::Error>> {
    role.commit().await?;
    let world = World::builder().build()?;
    let mut harness = Harness::new(&world);
    harness.connector = FixedConnector::empty();
    let root = role.root().as_path();
    let target = if relative {
        let parent = root.parent().ok_or("no parent")?;
        harness.cwd = super::support::host_path(parent)?;
        root.file_name()
            .ok_or("no name")?
            .to_string_lossy()
            .into_owned()
    } else {
        root.display().to_string()
    };
    let ran = harness.run(&["role", "check", &target]).await?;
    Ok((ran, harness, world))
}

#[tokio::test]
async fn a_valid_role_passes_with_no_config_no_gateway_and_a_relative_path() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        let (ran, harness, _world) = checked(&role, true).await?;
        ran.ok()?;
        assert!(ran.stdout.is_empty());
        assert_eq!(harness.config_bytes()?, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_role_fails_naming_the_field_the_stage_or_the_rule() -> TestResult {
    guard(async {
        let unknown = RoleFixture::new("sentinel")?
            .manifest_text("schema = 1\nmystery_field = 2\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n".to_owned());
        let (ran, _, _) = checked(&unknown, false).await?;
        failed(&ran, ExitClass::Failure, &["mystery_field"])?;

        let stage = RoleFixture::new("sentinel")?.dockerfile("FROM ubuntu\n");
        let (ran, _, _) = checked(&stage, false).await?;
        failed(&ran, ExitClass::Failure, &["construct"])?;

        let rule = RoleFixture::new("sentinel")?.policy_yaml(CODEX_RULE);
        let (ran, _, _) = checked(&rule, false).await?;
        failed(&ran, ExitClass::Failure, &["stolen-chat"])?;
        Ok(())
    })
    .await
}
