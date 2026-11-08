// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{DockerLayer, HookKind, ImageId, Platform, WorkspaceName};
use jackin_image::{ImagePlan, Rebuild};
use jackin_session::plan_image;
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
        let for_arm = plan_image(
            &setup.config,
            &setup.role,
            &arm,
            Some(workspace),
            Rebuild::Yes,
        )?;
        let for_amd = plan_image(
            &setup.config,
            &setup.role,
            &amd,
            Some(workspace),
            Rebuild::Yes,
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
        assert_eq!(for_arm.read_write, ["/tmp", "/sandbox"]);

        let build_only = plan_image(&setup.config, &setup.role, &arm, None, Rebuild::No)?;
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
        let request = plan_image(&setup.config, &dirty, &arm, None, Rebuild::No)?;
        assert_ne!(request.revision.digest, setup.role.revision().digest);
        assert_eq!(&request.revision, dirty.revision());
        Ok(())
    })
    .await
}
