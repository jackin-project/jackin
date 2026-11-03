// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, DockerCapability, DockerError, DockerLayer, DockerSource, SandboxPath};
use jackin_openshell::{
    EndpointProtocol, HostPattern, NetworkRule, ProviderProfile, RuleEndpoint, TlsMode,
};
use jackin_session::{LaunchWarning, PlanError};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, built_image, guard};

fn keys(rules: &[NetworkRule]) -> Vec<String> {
    let mut keys: Vec<String> = rules.iter().map(|rule| rule.key.to_string()).collect();
    keys.sort();
    keys
}

#[tokio::test]
async fn docker_is_off_by_default_and_adds_exactly_one_rule_one_layer_and_one_warning_when_enabled()
-> TestResult {
    guard(async {
        let rows = [
            (false, false, None),
            (true, false, Some(DockerSource::Role)),
            (false, true, Some(DockerSource::Workspace)),
            (true, true, Some(DockerSource::Both)),
        ];
        for (role_enabled, workspace_enabled, source) in rows {
            let mut spec = Spec::new(RoleFixture::new("sentinel")?.docker(role_enabled));
            spec.engine = Some(2376);
            spec.workspace_docker = workspace_enabled;
            let setup = spec.build().await?;
            let plan = setup.plan(setup.request(1)?)?;
            let created = plan.draft().complete(&built_image(&setup.role, '4')?, jackin_core::DockerLabel::Off);
            let role_rules = setup.role.policy().network_rules();
            let rules = created.policy().network_rules();
            match source {
                None => {
                    assert_eq!(plan.docker(), &DockerCapability::Disabled);
                    assert_eq!(plan.image_request().docker, DockerLayer::Absent);
                    assert!(plan.warnings().is_empty());
                    assert_eq!(rules, role_rules);
                }
                Some(expected) => {
                    assert!(
                        matches!(plan.docker(), DockerCapability::Enabled(access) if access.enabled_by == expected),
                        "{:?}",
                        plan.docker()
                    );
                    assert_eq!(plan.image_request().docker, DockerLayer::Client);
                    assert_eq!(plan.warnings(), [LaunchWarning::DockerCapability]);
                    assert_eq!(rules.len(), role_rules.len() + 1);
                    let mut wanted = keys(&role_rules);
                    wanted.push("docker_engine".to_owned());
                    wanted.sort();
                    assert_eq!(keys(&rules), wanted);
                    let engine = rules
                        .iter()
                        .find(|rule| rule.key.as_str() == "docker_engine")
                        .ok_or("no docker_engine rule")?;
                    assert_eq!(
                        engine.endpoints,
                        [RuleEndpoint {
                            host: HostPattern::parse("host.openshell.internal")?,
                            port: 2376,
                            protocol: EndpointProtocol::Tcp,
                            tls: TlsMode::Skip,
                        }]
                    );
                    assert_eq!(
                        engine.binaries,
                        [SandboxPath::parse("/usr/local/bin/docker")?]
                    );
                }
            }
            for asset in Agent::Codex.spec().profiles() {
                let profile = ProviderProfile::parse(asset)?;
                assert!(
                    created
                        .policy()
                        .closure_violations(&profile, &[])
                        .is_empty()
                );
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn enabled_docker_without_an_engine_is_a_typed_plan_error() -> TestResult {
    guard(async {
        for (role_enabled, workspace_enabled, source) in [
            (true, false, DockerSource::Role),
            (false, true, DockerSource::Workspace),
        ] {
            let mut spec = Spec::new(RoleFixture::new("sentinel")?.docker(role_enabled));
            spec.workspace_docker = workspace_enabled;
            let setup = spec.build().await?;
            let result = setup.plan(setup.request(1)?);
            assert!(
                matches!(
                    &result,
                    Err(PlanError::Docker(DockerError::EngineMissing { enabled_by }))
                        if *enabled_by == source
                ),
                "{:?}",
                result.err()
            );
        }
        Ok(())
    })
    .await
}

fn tag(
    plan: &jackin_session::LaunchPlan,
) -> Result<jackin_core::ImageTag, Box<dyn std::error::Error>> {
    let construct = jackin_core::ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    Ok(jackin_image::ImagePlan::new(plan.image_request())?.final_tag(&construct))
}

fn without_engine(text: &str) -> String {
    text.lines()
        .filter(|line| {
            !line.contains("docker_engine")
                && !line.starts_with("port = ")
                && !line.starts_with("client_certs")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn the_image_tag_follows_the_capability_and_never_the_engine_port_certificate_directory_or_source()
-> TestResult {
    guard(async {
        let mut spec = Spec::new(RoleFixture::new("sentinel")?);
        spec.engine = Some(2376);
        spec.workspace_docker = true;
        let enabled = spec.build().await?;
        let base = tag(&enabled.plan(enabled.request(1)?)?)?;
        let text = enabled.config.render();
        for (label, edited) in [
            ("port", text.replace("port = 2376", "port = 2999")),
            (
                "certificate directory",
                text.replace("/tmp/engine-certs", "/tmp/other-certs"),
            ),
        ] {
            let config = jackin_core::Config::parse(&edited)?;
            let plan = jackin_session::plan_launch(&config, &enabled.role, enabled.request(1)?)?;
            assert_eq!(tag(&plan)?, base, "{label}");
        }
        let off = text.replace("docker = true", "docker = false");
        let disabled = jackin_core::Config::parse(&off)?;
        let with_engine =
            jackin_session::plan_launch(&disabled, &enabled.role, enabled.request(1)?)?;
        let bare = jackin_core::Config::parse(&without_engine(&off))?;
        let without = jackin_session::plan_launch(&bare, &enabled.role, enabled.request(1)?)?;
        assert_eq!(tag(&with_engine)?, tag(&without)?);
        assert_ne!(tag(&with_engine)?, base);

        let mut by_role = Spec::new(RoleFixture::new("sentinel")?.docker(true));
        by_role.engine = Some(2376);
        let by_role = by_role.build().await?;
        let role_plan = by_role.plan(by_role.request(1)?)?;
        let workspace_plan = enabled.plan(enabled.request(1)?)?;
        assert_eq!(
            role_plan.image_request().docker,
            workspace_plan.image_request().docker
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn plan_image_and_plan_launch_agree_on_the_layer_for_the_four_flag_pairs() -> TestResult {
    guard(async {
        for (role_enabled, workspace_enabled) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let mut spec = Spec::new(RoleFixture::new("sentinel")?.docker(role_enabled));
            spec.engine = Some(2376);
            spec.workspace_docker = workspace_enabled;
            let setup = spec.build().await?;
            let (_, workspace) = setup.config.workspace_for(setup.project.path())?;
            let plan = setup.plan(setup.request(1)?)?;
            let image = jackin_session::plan_image(
                &setup.config,
                &setup.role,
                &setup.gateway,
                Some(workspace),
                jackin_image::Rebuild::No,
            )?;
            assert_eq!(image.docker, plan.image_request().docker);
            let build = jackin_session::plan_image(
                &setup.config,
                &setup.role,
                &setup.gateway,
                None,
                jackin_image::Rebuild::No,
            )?;
            let wanted = if role_enabled {
                DockerLayer::Client
            } else {
                DockerLayer::Absent
            };
            assert_eq!(build.docker, wanted, "{role_enabled} {workspace_enabled}");
        }
        Ok(())
    })
    .await
}
