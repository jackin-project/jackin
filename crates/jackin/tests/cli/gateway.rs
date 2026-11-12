// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::{ExitClass, GatewayLs};
use jackin_core::{GatewayName, OpenShellWorkspace, Platform};
use jackin_testkit::{TestResult, World, guard};

use super::support::{Failure, Harness, failed};

fn gateway(name: &str) -> Result<GatewayName, Failure> {
    Ok(GatewayName::parse(name)?)
}

#[tokio::test]
async fn gateway_add_stores_every_flag_and_the_host_platform_by_default() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness
            .run(&[
                "gateway",
                "add",
                "alpha",
                "--workspace",
                "ws1",
                "--builder",
                "bld",
                "--platform",
                "linux/amd64",
                "--engine-port",
                "2376",
                "--engine-certs",
                "certs",
                "--default",
            ])
            .await?
            .ok()?;
        harness.run(&["gateway", "add", "beta"]).await?.ok()?;

        let config = harness.stored().await?;
        let alpha = config.gateway(&gateway("alpha")?).ok_or("no alpha")?;
        assert_eq!(alpha.openshell_workspace, OpenShellWorkspace::parse("ws1")?);
        assert_eq!(alpha.builder.as_str(), "bld");
        assert_eq!(alpha.platform, Platform::LinuxAmd64);
        let engine = alpha.docker_engine.as_ref().ok_or("no engine")?;
        assert_eq!(engine.port.get(), 2376);
        assert_eq!(
            engine.client_certs.as_path(),
            world.home().as_path().join("certs")
        );
        assert_eq!(config.default_gateway(), Some(&gateway("alpha")?));

        let beta = config.gateway(&gateway("beta")?).ok_or("no beta")?;
        let host = if cfg!(target_arch = "aarch64") {
            Platform::LinuxArm64
        } else {
            Platform::LinuxAmd64
        };
        assert_eq!(beta.platform, host);
        assert_eq!(beta.openshell_workspace.as_str(), "default");
        assert_eq!(beta.builder.as_str(), "default");
        assert!(beta.docker_engine.is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_ls_json_has_a_pinned_shape_and_an_empty_list_is_a_literal() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let empty = harness.run(&["gateway", "ls", "--json"]).await?;
        empty.ok()?;
        assert_eq!(empty.stdout, "{\"schema\":1,\"gateways\":[]}\n");
        assert_eq!(harness.config_bytes()?, None);

        harness
            .run(&[
                "gateway",
                "add",
                "alpha",
                "--workspace",
                "ws1",
                "--builder",
                "bld",
                "--platform",
                "linux/amd64",
                "--engine-port",
                "2376",
                "--engine-certs",
                "/certs",
                "--default",
            ])
            .await?
            .ok()?;
        harness
            .run(&["gateway", "add", "beta", "--platform", "linux/arm64"])
            .await?
            .ok()?;
        let ran = harness.run(&["gateway", "ls", "--json"]).await?;
        ran.ok()?;
        assert_eq!(
            ran.stdout,
            concat!(
                "{\"schema\":1,\"gateways\":[",
                "{\"name\":\"alpha\",\"default\":true,\"workspace\":\"ws1\",\"builder\":\"bld\",",
                "\"platform\":\"linux/amd64\",\"engine\":{\"port\":2376,\"certs\":\"/certs\"}},",
                "{\"name\":\"beta\",\"default\":false,\"workspace\":\"default\",\"builder\":\"default\",",
                "\"platform\":\"linux/arm64\",\"engine\":null}]}\n"
            )
        );
        let typed: GatewayLs = ran.json()?;
        assert_eq!(typed.gateways.len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_rm_clears_a_default_and_duplicates_and_unknown_names_fail_without_a_write()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness
            .run(&["gateway", "add", "alpha", "--default"])
            .await?
            .ok()?;
        harness.run(&["gateway", "add", "beta"]).await?.ok()?;
        let before = harness.config_bytes()?;

        let duplicate = harness.run(&["gateway", "add", "alpha"]).await?;
        failed(&duplicate, ExitClass::Failure, &["alpha"])?;
        let unknown = harness.run(&["gateway", "rm", "zeta"]).await?;
        failed(&unknown, ExitClass::Failure, &["zeta"])?;
        assert_eq!(harness.config_bytes()?, before);

        harness.run(&["gateway", "rm", "alpha"]).await?.ok()?;
        let config = harness.stored().await?;
        assert_eq!(config.default_gateway(), None);
        assert!(config.gateway(&gateway("alpha")?).is_none());
        assert!(config.gateway(&gateway("beta")?).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_add_refuses_the_plaintext_docker_port_and_leaves_the_config_unchanged()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let refused = harness
            .run(&[
                "gateway",
                "add",
                "beta",
                "--engine-port",
                "2375",
                "--engine-certs",
                "certs",
            ])
            .await?;
        failed(
            &refused,
            ExitClass::Failure,
            &["beta", "docker_engine.port", "2375"],
        )?;
        assert_eq!(harness.config_bytes()?, None);
        let empty = harness.run(&["gateway", "ls"]).await?;
        empty.ok()?;
        assert!(empty.stdout.is_empty());

        harness
            .run(&["gateway", "add", "alpha", "--default"])
            .await?
            .ok()?;
        let before = harness.config_bytes()?;
        let again = harness
            .run(&[
                "gateway",
                "add",
                "beta",
                "--engine-port",
                "2375",
                "--engine-certs",
                "certs",
                "--default",
            ])
            .await?;
        failed(&again, ExitClass::Failure, &["beta", "docker_engine.port"])?;
        assert_eq!(harness.config_bytes()?, before);
        let listed = harness.run(&["gateway", "ls"]).await?;
        listed.ok()?;
        assert!(listed.stdout.starts_with("alpha\t"), "{}", listed.stdout);
        assert!(!listed.stdout.contains("beta"), "{}", listed.stdout);

        harness
            .run(&[
                "gateway",
                "add",
                "gamma",
                "--engine-port",
                "2376",
                "--engine-certs",
                "certs",
            ])
            .await?
            .ok()?;
        let listed = harness.run(&["gateway", "ls"]).await?;
        listed.ok()?;
        assert!(listed.stdout.contains("gamma\t"), "{}", listed.stdout);
        Ok(())
    })
    .await
}
