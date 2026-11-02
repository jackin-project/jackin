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
            let created = plan.draft().complete(&built_image(&setup.role, '4')?);
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
