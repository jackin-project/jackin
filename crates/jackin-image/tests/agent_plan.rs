// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, Platform, Provider, RepoPath, RoleName, Sha256,
    TupleState, lookup_tuple,
};
use jackin_image::{HookFiles, ImagePlan, ImageRequest, Rebuild, RoleRevision, SourceFile};

type TestResult = Result<(), Box<dyn Error>>;

fn request(agent: Agent) -> Result<ImageRequest, Box<dyn Error>> {
    Ok(ImageRequest {
        role: RoleName::parse("sentinel")?,
        context_files: vec![],
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: b"FROM construct AS role\n".to_vec(),
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision {
            commit: CommitId::parse(&"1".repeat(40))?,
            digest: Sha256::of(b"clean"),
        },
        agent,
        agents: vec![agent],
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        rebuild: Rebuild::No,
    })
}

fn args(agent: Agent) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let plan = ImagePlan::new(&request(agent)?)?;
    let id = jackin_core::ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    Ok(plan
        .final_builds(&id)
        .derived
        .args
        .iter()
        .filter_map(|arg| {
            arg.strip_prefix("--build-arg=")?
                .split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        })
        .collect())
}

#[test]
fn omp_plan_names_omp_binary_and_version() -> TestResult {
    let map = args(Agent::Omp)?;
    let spec = Agent::Omp.spec();
    assert_eq!(
        map.get("AGENT_BINARY").map(String::as_str),
        Some(spec.binary.as_str())
    );
    assert_eq!(
        map.get("AGENT_VERSION").map(String::as_str),
        Some(spec.install.version)
    );
    for target in spec.install.targets {
        let name = format!(
            "AGENT_SHA256_{}",
            target.platform.target_arch().to_ascii_uppercase()
        );
        assert_eq!(
            map.get(&name).map(String::as_str),
            Some(target.sha256),
            "{name}"
        );
    }
    let tuple = lookup_tuple(Agent::Omp, Provider::Zai)?;
    assert_eq!(tuple.state, TupleState::Unavailable);
    Ok(())
}

#[test]
fn codex_plan_unchanged() -> TestResult {
    let map = args(Agent::Codex)?;
    let spec = Agent::Codex.spec();
    assert_eq!(
        map.get("AGENT_BINARY").map(String::as_str),
        Some(spec.binary.as_str())
    );
    assert_eq!(
        map.get("AGENT_VERSION").map(String::as_str),
        Some(spec.install.version)
    );
    Ok(())
}
