// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{DockerLayer, HookKind, ImageId, Platform, SandboxPath, WorkspaceName};
use jackin_image::{ImagePlan, Rebuild};
use jackin_session::plan_image_for;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

const GATEWAYS: &str = "default_gateway = \"gw-arm\"\n\n[gateways.gw-arm]\nopenshell_workspace = \"default\"\nbuilder = \"arm-builder\"\nplatform = \"linux/arm64\"\n\n[gateways.gw-amd]\nopenshell_workspace = \"default\"\nbuilder = \"amd-builder\"\nplatform = \"linux/amd64\"\n";

#[tokio::test]
async fn the_image_request_follows_gateway_role_and_workspace_inputs() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.hook(HookKind::Setup, b"#!/bin/sh\necho setup\n");
        let mut spec = Spec::new(role);
        spec.gateways = Some(GATEWAYS.to_owned());
        spec.workspace_docker = true;
        let setup = spec.build().await?;
        let workspace = setup
            .config
            .workspace(&WorkspaceName::parse("platform-backend")?)
            .ok_or("no workspace")?;
        let arm = jackin_core::GatewayName::parse("gw-arm")?;
        let amd = jackin_core::GatewayName::parse("gw-amd")?;
        let for_arm = plan_image_for(
            &setup.config,
            &setup.role,
            &arm,
            Some(workspace),
            Rebuild::Yes,
            jackin_core::Agent::Codex,
        )?;
        let for_amd = plan_image_for(
            &setup.config,
            &setup.role,
            &amd,
            Some(workspace),
            Rebuild::Yes,
            jackin_core::Agent::Codex,
        )?;
        assert_eq!(for_arm.platform, Platform::LinuxArm64);
        assert_eq!(for_arm.builder.as_str(), "arm-builder");
        assert_eq!(for_amd.platform, Platform::LinuxAmd64);
        assert_eq!(for_amd.builder.as_str(), "amd-builder");
        let setup_hook = for_arm.hooks.setup.as_ref().ok_or("no setup hook")?;
        assert_eq!(setup_hook.bytes, b"#!/bin/sh\necho setup\n");
        assert_eq!(setup_hook.path.as_str(), "hooks/setup.sh");
        assert!(for_arm.hooks.preflight.is_none());
        assert_eq!(&for_arm.revision, setup.role.revision());
        assert_eq!(for_arm.rebuild, Rebuild::Yes);
        assert_eq!(for_arm.docker, DockerLayer::Client);

        let build_only = plan_image_for(
            &setup.config,
            &setup.role,
            &arm,
            None,
            Rebuild::No,
            jackin_core::Agent::Codex,
        )?;
        assert_eq!(build_only.docker, DockerLayer::Absent);
        assert_eq!(build_only.rebuild, Rebuild::No);

        let construct = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
        let arm_tag = ImagePlan::new(&for_arm)?.final_tag(&construct);
        let amd_tag = ImagePlan::new(&for_amd)?.final_tag(&construct);
        assert_ne!(arm_tag, amd_tag);

        setup.role_fixture.file("notes.txt", b"dirty\n").await?;
        let (catalog, _runner) = super::support::catalog(&setup.cache);
        let dirty = catalog
            .load_path(
                setup.role_fixture.root(),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await?;
        let request = plan_image_for(
            &setup.config,
            &dirty,
            &arm,
            None,
            Rebuild::No,
            jackin_core::Agent::Codex,
        )?;
        assert_ne!(request.revision.digest, setup.role.revision().digest);
        assert_eq!(&request.revision, dirty.revision());
        Ok(())
    })
    .await
}

fn policy_with(read_write: &[&str]) -> String {
    let lines = read_write
        .iter()
        .map(|path| format!("    - '{}'\n", path.replace('\'', "''")))
        .collect::<Vec<_>>()
        .concat();
    format!(
        "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n{lines}landlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  github:\n    name: github\n    endpoints:\n      - host: github.com\n        port: 443\n    binaries:\n      - path: /usr/bin/git\n"
    )
}

#[tokio::test]
async fn the_image_request_carries_the_normalized_read_write_paths_of_the_role_policy() -> TestResult
{
    guard(async {
        let construct = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
        let cases: [(&[&str], &[&str]); 4] = [
            (&["/tmp", "/sandbox"], &["/dev/null", "/sandbox", "/tmp"]),
            (
                &["//var/./data/", "/sandbox", "/sandbox/"],
                &["/dev/null", "/sandbox", "/tmp", "/var/data"],
            ),
            (
                &["/sandbox/tools"],
                &["/dev/null", "/sandbox/tools", "/tmp"],
            ),
            (
                &["/sandbox", "/opt/with space/it's"],
                &["/dev/null", "/opt/with space/it's", "/sandbox", "/tmp"],
            ),
        ];
        let mut tags = Vec::new();
        for (listed, wanted) in cases {
            let role = RoleFixture::new("sentinel")?.policy_yaml(&policy_with(listed));
            let setup = Spec::new(role).build().await?;
            let request = plan_image_for(
                &setup.config,
                &setup.role,
                &setup.gateway,
                None,
                Rebuild::No,
                jackin_core::Agent::Codex,
            )?;
            let wanted: Vec<SandboxPath> = wanted
                .iter()
                .map(|path| SandboxPath::parse(path))
                .collect::<Result<_, _>>()?;
            assert_eq!(request.read_write, wanted, "{listed:?}");
            tags.push(ImagePlan::new(&request)?.final_tag(&construct));
        }
        let distinct: std::collections::BTreeSet<_> = tags.iter().collect();
        assert_eq!(distinct.len(), tags.len(), "{tags:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_policy_with_no_writable_path_at_all_still_plans_an_image() -> TestResult {
    guard(async {
        let construct = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
        let policy = policy_with(&[])
            .replace("include_workdir: true", "include_workdir: false")
            .replace("    - /usr\n", "    - /usr\n    - /tmp\n    - /dev/null\n")
            .replace("  read_write:\n", "  read_write: []\n");
        let role = RoleFixture::new("sentinel")?.policy_yaml(&policy);
        let setup = Spec::new(role).build().await?;
        let request = plan_image_for(
            &setup.config,
            &setup.role,
            &setup.gateway,
            None,
            Rebuild::No,
            jackin_core::Agent::Codex,
        )?;
        assert!(request.read_write.is_empty(), "{:?}", request.read_write);
        let plan = ImagePlan::new(&request)?;
        let with_tmp = {
            let role = RoleFixture::new("sentinel")?.policy_yaml(&policy_with(&["/tmp"]));
            let setup = Spec::new(role).build().await?;
            plan_image_for(
                &setup.config,
                &setup.role,
                &setup.gateway,
                None,
                Rebuild::No,
                jackin_core::Agent::Codex,
            )?
        };
        assert_ne!(
            plan.final_tag(&construct),
            ImagePlan::new(&with_tmp)?.final_tag(&construct)
        );
        Ok(())
    })
    .await
}
