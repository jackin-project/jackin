// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Agent, DOCKER_CLIENT, DockerLayer, DockerPart, ImageId, InstallTarget, Platform, SandboxPath,
    Sha256,
};

use crate::assets::{DERIVED_DOCKERFILE, JACKIN_START, SCAN};
use crate::error::ImageError;
use crate::request::{HookFiles, ImageRequest, RoleRevision, SourceFile};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey(Sha256);

#[derive(Debug, Default)]
pub(crate) struct Fields(Vec<u8>);

impl Fields {
    pub(crate) fn push(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        self.0.extend((bytes.len() as u64).to_be_bytes());
        self.0.extend(bytes);
    }

    pub(crate) fn digest(&self) -> Sha256 {
        Sha256::of(&self.0)
    }
}

const PLATFORMS: [Platform; 2] = [Platform::LinuxArm64, Platform::LinuxAmd64];

pub(crate) fn install_target(
    agent: Agent,
    platform: Platform,
) -> Result<&'static InstallTarget, ImageError> {
    agent
        .spec()
        .install_target(platform)
        .ok_or(ImageError::MissingInstallTarget { platform })
}

fn parent_directory(path: &SandboxPath) -> &str {
    match path.as_str().rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((directory, _)) => directory,
    }
}

fn build_arg(name: &str, value: &str) -> String {
    format!("--build-arg={name}={value}")
}

fn docker_checksum(part: &DockerPart, platform: Platform) -> Result<&'static str, ImageError> {
    part.target(platform)
        .map(|target| target.sha256)
        .ok_or(ImageError::MissingInstallTarget { platform })
}

fn docker_arguments(args: &mut Vec<String>) -> Result<(), ImageError> {
    args.push(build_arg("DOCKER_LAYER", "client"));
    let parts = [
        ("CLI", &DOCKER_CLIENT.cli),
        ("BUILDX", &DOCKER_CLIENT.buildx),
        ("COMPOSE", &DOCKER_CLIENT.compose),
    ];
    for (name, part) in parts {
        args.push(build_arg(&format!("DOCKER_{name}_VERSION"), part.version));
    }
    for (name, part) in parts {
        for platform in PLATFORMS {
            let arg = format!(
                "DOCKER_{name}_SHA256_{}",
                platform.target_arch().to_ascii_uppercase()
            );
            args.push(build_arg(&arg, docker_checksum(part, platform)?));
        }
    }
    Ok(())
}

pub(crate) fn derived_arguments_multi(
    agents: &[Agent],
    docker: DockerLayer,
) -> Result<Vec<String>, ImageError> {
    let mut sorted = agents.to_vec();
    sorted.sort_by_key(|agent| agent.as_str());
    sorted.dedup();
    if sorted.is_empty() {
        sorted.push(Agent::Codex);
    }
    let mut args = derived_arguments_prefixed(sorted[0], docker, "AGENT")?;
    if sorted.len() == 1 {
        return Ok(args);
    }
    args.push(build_arg(
        "AGENTS",
        &sorted
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(","),
    ));
    for (index, agent) in sorted.iter().enumerate().skip(1) {
        let prefix = format!("AGENT{}", index + 1);
        args.extend(derived_arguments_prefixed(
            *agent,
            DockerLayer::Absent,
            &prefix,
        )?);
    }
    Ok(args)
}

pub(crate) fn derived_arguments_prefixed(
    agent: Agent,
    docker: DockerLayer,
    prefix: &str,
) -> Result<Vec<String>, ImageError> {
    let spec = agent.spec();
    let mut args = vec![
        build_arg(&format!("{prefix}_VERSION"), spec.install.version),
        build_arg(&format!("{prefix}_BIN_DIR"), parent_directory(&spec.binary)),
        build_arg(&format!("{prefix}_BINARY"), spec.binary.as_str()),
        build_arg(&format!("{prefix}_NAME"), agent.as_str()),
        build_arg(
            &format!("{prefix}_ARCHIVE_MEMBERS"),
            &spec.install.archive_members.join(" "),
        ),
        build_arg(
            &format!("{prefix}_VERSION_OUTPUT"),
            &spec
                .install
                .version_output
                .replace("{version}", spec.install.version),
        ),
        build_arg(
            &format!("{prefix}_SCAN_ABSENT"),
            &spec.install.scan_absent_paths.join(" "),
        ),
        build_arg(
            &format!("{prefix}_EXTRA_BINARY"),
            if agent == Agent::Codex {
                spec.codex.code_mode_host.as_str()
            } else {
                ""
            },
        ),
    ];
    if !spec.install.system_config.as_str().is_empty() {
        args.insert(
            6,
            build_arg(
                &format!("{prefix}_SYSTEM_CONFIG"),
                spec.install.system_config.as_str(),
            ),
        );
        args.insert(
            7,
            build_arg(
                &format!("{prefix}_SYSTEM_CONFIG_CONTENT"),
                spec.install.system_config_content,
            ),
        );
    }
    for platform in PLATFORMS {
        let target = install_target(agent, platform)?;
        let name = format!(
            "{prefix}_SHA256_{}",
            platform.target_arch().to_ascii_uppercase()
        );
        args.push(build_arg(&name, target.sha256));
        let url_name = format!(
            "{prefix}_URL_{}",
            platform.target_arch().to_ascii_uppercase()
        );
        let url = spec
            .install
            .url_template
            .replace("{version}", spec.install.version)
            .replace("{target}", target.triple);
        args.push(build_arg(&url_name, &url));
    }
    if docker == DockerLayer::Client {
        docker_arguments(&mut args)?;
    }
    Ok(args)
}

impl ImageKey {
    pub fn derive(request: &ImageRequest, construct: &ImageId) -> Result<ImageKey, ImageError> {
        let derived_args = derived_arguments_multi(&request.sorted_agents(), request.docker)?;
        Ok(ImageKey::compute(request, &derived_args, construct))
    }

    pub fn sha256(&self) -> &Sha256 {
        &self.0
    }

    pub(crate) fn compute(
        request: &ImageRequest,
        derived_args: &[String],
        construct: &ImageId,
    ) -> ImageKey {
        let ImageRequest {
            role,
            context_files: _,
            dockerfile,
            hooks,
            revision,
            agent,
            agents,
            platform,
            builder: _,
            docker,
            rebuild: _,
        } = request;
        let SourceFile {
            path: _,
            bytes: dockerfile_bytes,
        } = dockerfile;
        let HookFiles { setup, preflight } = hooks;
        let mut fields = Fields::default();
        fields.push(role.as_str());
        fields.push(agent.as_str());
        for sorted_agent in {
            let mut v = agents.clone();
            v.sort_by_key(|a| a.as_str());
            v.dedup();
            if v.len() <= 1 {
                v.clear();
            }
            v
        } {
            fields.push(sorted_agent.as_str());
            fields.push(sorted_agent.spec().install.version);
            for platform_item in PLATFORMS {
                if let Some(target) = sorted_agent.spec().install_target(platform_item) {
                    fields.push(target.sha256);
                }
            }
        }
        let RoleRevision { commit, digest } = revision;
        fields.push(commit.as_str());
        fields.push(digest.as_str());
        fields.push(Sha256::of(dockerfile_bytes).as_str());
        fields.push(construct.to_string());
        fields.push(derived_args.len().to_string());
        for arg in derived_args {
            fields.push(arg);
        }
        fields.push(platform.docker_arg());
        fields.push(match docker {
            DockerLayer::Absent => "absent",
            DockerLayer::Client => "client",
        });
        fields.push(Sha256::of(JACKIN_START).as_str());
        fields.push(Sha256::of(DERIVED_DOCKERFILE).as_str());
        fields.push(Sha256::of(SCAN).as_str());
        for hook in [setup, preflight] {
            if let Some(file) = hook {
                fields.push(file.path.as_str());
                fields.push(Sha256::of(&file.bytes).as_str());
            } else {
                fields.push("");
                fields.push("");
            }
        }
        ImageKey(fields.digest())
    }
}
