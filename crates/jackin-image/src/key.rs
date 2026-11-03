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

pub(crate) fn derived_arguments(
    agent: Agent,
    docker: DockerLayer,
) -> Result<Vec<String>, ImageError> {
    let spec = agent.spec();
    let mut args = vec![
        build_arg("CODEX_VERSION", spec.install.version),
        build_arg("CODEX_BIN_DIR", parent_directory(&spec.binary)),
    ];
    for platform in PLATFORMS {
        let target = install_target(agent, platform)?;
        let name = format!(
            "CODEX_SHA256_{}",
            platform.target_arch().to_ascii_uppercase()
        );
        args.push(build_arg(&name, target.sha256));
    }
    if docker == DockerLayer::Client {
        docker_arguments(&mut args)?;
    }
    Ok(args)
}

impl ImageKey {
    pub fn derive(request: &ImageRequest, construct: &ImageId) -> Result<ImageKey, ImageError> {
        let derived_args = derived_arguments(request.agent, request.docker)?;
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
