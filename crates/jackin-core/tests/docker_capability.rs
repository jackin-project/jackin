// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    Config, DockerCapability, DockerError, DockerLayer, DockerRisk, DockerSource, EngineAccess,
    EngineConfig, EnvName, GatewayName, RoleManifest,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn manifest(enabled: bool) -> String {
    format!(
        "schema = 1\n[role]\nname = \"r\"\ndockerfile = \"Dockerfile\"\npolicy = \"p.yaml\"\n[docker]\nenabled = {enabled}\n"
    )
}

fn config(workspace_docker: bool) -> String {
    format!(
        "[gateways.with]\nopenshell_workspace = \"w\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n[gateways.with.docker_engine]\nport = 18476\nclient_certs = \"/certs\"\n[gateways.without]\nopenshell_workspace = \"w\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n[workspaces.w]\npath = \"/p\"\ndocker = {workspace_docker}\n"
    )
}

fn source_label(source: DockerSource) -> &'static str {
    match source {
        DockerSource::Role => "role",
        DockerSource::Workspace => "workspace",
        DockerSource::Both => "both",
    }
}

#[test]
fn docker_capability_resolves_from_parsed_manifest_workspace_and_gateway() -> TestResult {
    let expected_env: BTreeMap<EnvName, String> = [
        ("DOCKER_HOST", "tcp://host.openshell.internal:18476"),
        ("DOCKER_TLS_VERIFY", "1"),
        ("DOCKER_CERT_PATH", "/sandbox/.jackin/docker"),
    ]
    .into_iter()
    .map(|(name, value)| Ok((EnvName::parse(name)?, value.to_owned())))
    .collect::<Result<_, jackin_core::IdError>>()?;
    for name in expected_env.keys() {
        assert!(name.is_reserved(), "{name}");
    }
    for role in [false, true] {
        for workspace in [false, true] {
            let parsed_role = RoleManifest::parse(&manifest(role))?;
            let parsed = Config::parse(&config(workspace))?;
            let flag = parsed
                .workspaces()
                .next()
                .map(|(_, w)| w.docker)
                .ok_or("workspace")?;
            let with = parsed.gateway(&GatewayName::parse("with")?).ok_or("with")?;
            let without = parsed
                .gateway(&GatewayName::parse("without")?)
                .ok_or("without")?;
            let engine = with.docker_engine.as_ref().ok_or("engine")?;
            assert!(without.docker_engine.is_none());
            for engine_arg in [Some(engine), None] {
                let result =
                    DockerCapability::resolve(parsed_role.docker.enabled, flag, engine_arg);
                let source = match (role, workspace) {
                    (false, false) => None,
                    (true, false) => Some(DockerSource::Role),
                    (false, true) => Some(DockerSource::Workspace),
                    (true, true) => Some(DockerSource::Both),
                };
                match (source, engine_arg, result) {
                    (None, _, Ok(DockerCapability::Disabled)) => {}
                    (
                        Some(by),
                        Some(want),
                        Ok(DockerCapability::Enabled(EngineAccess { engine, enabled_by })),
                    ) => {
                        assert_eq!(enabled_by, by);
                        assert_eq!(engine, *want);
                    }
                    (Some(by), None, Err(DockerError::EngineMissing { enabled_by })) => {
                        assert_eq!(enabled_by, by);
                    }
                    (_, _, other) => {
                        return Err(format!(
                            "{role} {workspace} {}: {other:?}",
                            engine_arg.is_some()
                        )
                        .into());
                    }
                }
            }
            let disabled = DockerCapability::resolve(false, false, Some(engine))?;
            assert_eq!(disabled.layer(), DockerLayer::Absent);
            assert!(disabled.environment().is_empty());
            if role || workspace {
                let enabled = DockerCapability::resolve(role, workspace, Some(engine))?;
                assert_eq!(enabled.layer(), DockerLayer::Client);
                assert_eq!(enabled.environment(), expected_env);
                assert!(enabled.environment().keys().all(EnvName::is_reserved));
                let DockerCapability::Enabled(access) = enabled else {
                    return Err("enabled".into());
                };
                assert_eq!(
                    access.engine,
                    EngineConfig {
                        port: engine.port,
                        client_certs: engine.client_certs.clone()
                    }
                );
                assert_eq!(
                    source_label(access.enabled_by),
                    source_label(match (role, workspace) {
                        (true, true) => DockerSource::Both,
                        (true, false) => DockerSource::Role,
                        _ => DockerSource::Workspace,
                    })
                );
            }
        }
    }
    Ok(())
}

#[test]
fn docker_risk_all_lists_the_four_d24_warnings_once() {
    let index = |risk: DockerRisk| match risk {
        DockerRisk::ClientKeyReadable => 0,
        DockerRisk::ContainersBypassPolicy => 1,
        DockerRisk::BindMountsNameEngineHost => 2,
        DockerRisk::EngineOutlivesSandbox => 3,
    };
    assert_eq!(DockerRisk::ALL.len(), 4);
    let mut seen: Vec<usize> = DockerRisk::ALL.into_iter().map(index).collect();
    seen.sort_unstable();
    assert_eq!(seen, [0, 1, 2, 3]);
}

#[test]
fn docker_risk_text_is_the_approved_baseline_wording() {
    let wanted = [
        ("Client key", "The agent can read the engine client key."),
        ("Network", "Containers bypass the sandbox network policy."),
        (
            "Bind mounts",
            "A bind mount names a path on the engine host, not in the sandbox.",
        ),
        (
            "Lifetime",
            "The engine outlives the sandbox. Sandboxes that share one engine see each other's containers, and you remove them.",
        ),
    ];
    let actual: Vec<(&str, &str)> = DockerRisk::ALL
        .into_iter()
        .map(|risk| (risk.label(), risk.text()))
        .collect();
    assert_eq!(actual, wanted);
}
