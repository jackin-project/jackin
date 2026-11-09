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
    let tuple = lookup_tuple(Agent::Omp, Provider::Kimi)?;
    assert_eq!(tuple.state, TupleState::Supported);
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

fn derived_dockerfile(agent: Agent) -> Result<String, Box<dyn Error>> {
    let plan = ImagePlan::new(&request(agent)?)?;
    let id = jackin_core::ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    let built = plan.final_builds(&id);
    let jackin_image::ContextSource::Materialized(files) = built.derived.context else {
        return Err("derived context is not materialized".into());
    };
    let file = files
        .iter()
        .find(|file| file.name == "Dockerfile")
        .ok_or("derived Dockerfile")?;
    Ok(String::from_utf8(file.bytes.clone())?)
}

#[test]
fn codex_scan_absent_paths_unchanged() -> TestResult {
    let map = args(Agent::Codex)?;
    assert_eq!(
        map.get("AGENT_SCAN_ABSENT").map(String::as_str),
        Some(
            "/etc/codex/requirements.toml /etc/codex/managed_config.toml /etc/codex/hooks.json /etc/codex/rules /etc/codex/agents /etc/codex/skills"
        )
    );
    assert_eq!(
        derived_dockerfile(Agent::Codex)?,
        String::from_utf8(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/images/derived/Dockerfile"
            ))
            .to_vec()
        )?
    );
    Ok(())
}

#[test]
fn omp_scan_absent_paths_have_no_codex_paths() -> TestResult {
    let map = args(Agent::Omp)?;
    assert_eq!(map.get("AGENT_SCAN_ABSENT").map(String::as_str), Some(""));
    let text = derived_dockerfile(Agent::Omp)?;
    assert!(!text.contains("/etc/codex"), "{text}");
    let absent: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("--absent"))
        .collect();
    assert_eq!(absent.len(), 1, "{absent:?}");
    assert!(
        absent[0].contains("/usr/local/lib/jackin/image-scan"),
        "{absent:?}"
    );
    Ok(())
}

#[test]
fn omp_plan_has_no_system_config_entries() -> TestResult {
    let map = args(Agent::Omp)?;
    assert_eq!(map.get("AGENT_SYSTEM_CONFIG"), None);
    assert_eq!(map.get("AGENT_SYSTEM_CONFIG_CONTENT"), None);
    assert_eq!(Agent::Omp.spec().install.system_config.as_str(), "");
    assert_eq!(
        Agent::Omp.spec().user_config.path.as_str(),
        "/sandbox/.omp/config.toml"
    );
    let text = derived_dockerfile(Agent::Omp)?;
    assert!(!text.contains("/etc/codex"), "{text}");
    assert!(
        !text.contains("COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${AGENT_SYSTEM_CONFIG} ${AGENT_SYSTEM_CONFIG}"),
        "{text}"
    );
    assert!(
        !text.contains("--same-as ${AGENT_SYSTEM_CONFIG}=/trusted${AGENT_SYSTEM_CONFIG}"),
        "{text}"
    );
    assert!(!text.contains("--data ${AGENT_SYSTEM_CONFIG}"), "{text}");
    assert!(!text.contains("\"/out${AGENT_SYSTEM_CONFIG}\""), "{text}");
    assert!(
        !text.contains(&format!(
            "--data {}",
            Agent::Omp.spec().user_config.path.as_str()
        )),
        "{text}"
    );
    Ok(())
}

#[test]
fn two_agent_plan_installs_and_scans_both_binaries() -> TestResult {
    let mut req = request(Agent::Codex)?;
    req.agents = vec![Agent::Codex, Agent::Omp];
    let plan = ImagePlan::new(&req)?;
    let id = jackin_core::ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    let built = plan.final_builds(&id);
    let args = built
        .derived
        .args
        .iter()
        .filter_map(|arg| {
            arg.strip_prefix("--build-arg=")?
                .split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        })
        .collect::<BTreeMap<_, _>>();
    for (prefix, agent) in [("AGENT", Agent::Codex), ("AGENT2", Agent::Omp)] {
        let spec = agent.spec();
        assert_eq!(
            args.get(&format!("{prefix}_BINARY")).map(String::as_str),
            Some(spec.binary.as_str())
        );
        assert_eq!(
            args.get(&format!("{prefix}_VERSION")).map(String::as_str),
            Some(spec.install.version)
        );
        for target in spec.install.targets {
            let name = format!(
                "{prefix}_SHA256_{}",
                target.platform.target_arch().to_ascii_uppercase()
            );
            assert_eq!(
                args.get(&name).map(String::as_str),
                Some(target.sha256),
                "{name}"
            );
            let url = format!(
                "{prefix}_URL_{}",
                target.platform.target_arch().to_ascii_uppercase()
            );
            assert!(
                args.get(&url).is_some_and(|value| !value.is_empty()),
                "{url}"
            );
        }
    }
    let jackin_image::ContextSource::Materialized(files) = built.derived.context else {
        return Err("derived context is not materialized".into());
    };
    let dockerfile = files
        .iter()
        .find(|file| file.name == "Dockerfile")
        .ok_or("derived Dockerfile")?;
    let text = String::from_utf8(dockerfile.bytes.clone())?;
    assert!(text.contains("AGENT2_BINARY"), "{text}");
    assert!(
        text.contains("--same-as ${AGENT2_BINARY}=/trusted${AGENT2_BINARY}"),
        "{text}"
    );
    assert!(text.contains("agent2-archive"), "{text}");
    assert_eq!(
        args.get("AGENT_SYSTEM_CONFIG").map(String::as_str),
        Some("/etc/codex/config.toml")
    );
    assert_eq!(args.get("AGENT2_SYSTEM_CONFIG"), None);
    assert!(text.contains("--data ${AGENT_SYSTEM_CONFIG}"), "{text}");
    assert!(!text.contains("--data ${AGENT2_SYSTEM_CONFIG}"), "{text}");
    assert!(
        !text.contains("--same-as ${AGENT2_SYSTEM_CONFIG}=/trusted${AGENT2_SYSTEM_CONFIG}"),
        "{text}"
    );
    let mut changed = request(Agent::Codex)?;
    changed.agents = vec![Agent::Codex, Agent::MimoCode];
    let other = ImagePlan::new(&changed)?.final_builds(&id);
    assert_ne!(built.key.sha256(), other.key.sha256());
    let single = ImagePlan::new(&request(Agent::Codex)?)?.final_builds(&id);
    let jackin_image::ContextSource::Materialized(single_files) = single.derived.context else {
        return Err("single derived context is not materialized".into());
    };
    let single_dockerfile = single_files
        .iter()
        .find(|file| file.name == "Dockerfile")
        .ok_or("single Dockerfile")?;
    assert_eq!(
        single_dockerfile.bytes,
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/images/derived/Dockerfile"
        ))
        .to_vec()
    );
    Ok(())
}

#[test]
fn minimax_code_tuple_is_unavailable_and_image_plan_has_no_install_target() -> TestResult {
    let tuple = lookup_tuple(Agent::MinimaxCode, Provider::Minimax)?;
    assert_eq!(tuple.id, "T06");
    assert_eq!(tuple.state, TupleState::Unavailable);
    assert_eq!(tuple.reason, Some("no stable Linux artifact"));
    let err = ImagePlan::new(&request(Agent::MinimaxCode)?)
        .err()
        .ok_or("expected error")?;
    assert!(
        matches!(err, jackin_image::ImageError::MissingInstallTarget { .. }),
        "{err}"
    );
    Ok(())
}

#[test]
fn antigravity_plan_pins_the_stable_tarball_and_renames_the_member() -> TestResult {
    let tuple = lookup_tuple(Agent::Antigravity, Provider::Antigravity)?;
    assert_eq!(tuple.id, "T05");
    assert_eq!(tuple.state, TupleState::Unavailable);
    assert_eq!(
        tuple.reason,
        Some("subscription requires a desktop keyring in the sandbox")
    );
    assert_eq!(tuple.auth_class, jackin_core::AuthClass::BlockedVendor);
    assert_ne!(tuple.state, TupleState::Supported);
    assert_ne!(tuple.state, TupleState::BlockedVendor);
    let map = args(Agent::Antigravity)?;
    let spec = Agent::Antigravity.spec();
    assert_eq!(
        map.get("AGENT_ARCHIVE_MEMBERS").map(String::as_str),
        Some("antigravity")
    );
    assert_eq!(
        map.get("AGENT_BINARY").map(String::as_str),
        Some(spec.binary.as_str())
    );
    assert_eq!(
        map.get("AGENT_VERSION").map(String::as_str),
        Some(spec.install.version)
    );
    for (platform, url, member) in [
        (
            Platform::LinuxArm64,
            "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.3-6604829099950080/linux-arm/cli_linux_arm64.tar.gz",
            "arm/cli_linux_arm64.tar.gz",
        ),
        (
            Platform::LinuxAmd64,
            "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.3-6604829099950080/linux-x64/cli_linux_x64.tar.gz",
            "x64/cli_linux_x64.tar.gz",
        ),
    ] {
        let target = spec.install_target(platform).ok_or("missing platform")?;
        assert_eq!(target.triple, member);
        let arch = platform.target_arch().to_ascii_uppercase();
        let sha_name = format!("AGENT_SHA256_{arch}");
        assert_eq!(
            map.get(&sha_name).map(String::as_str),
            Some(target.sha256),
            "{sha_name}"
        );
        let url_name = format!("AGENT_URL_{arch}");
        assert_eq!(
            map.get(&url_name).map(String::as_str),
            Some(url),
            "{url_name}"
        );
    }
    let text = derived_dockerfile(Agent::Antigravity)?;
    assert!(
        text.contains("install -D -m 0755 -o root -g root \"$work/$1\" \"/out${AGENT_BINARY}\""),
        "{text}"
    );
    Ok(())
}
