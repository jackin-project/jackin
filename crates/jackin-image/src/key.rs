// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, DockerLayer, ImageId, InstallTarget, Platform, Sha256};

use crate::assets::{DERIVED_DOCKERFILE, JACKIN_START};
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

pub(crate) fn install_target(
    agent: Agent,
    platform: Platform,
) -> Result<&'static InstallTarget, ImageError> {
    agent
        .spec()
        .install_target(platform)
        .ok_or(ImageError::MissingInstallTarget { platform })
}

impl ImageKey {
    pub fn derive(request: &ImageRequest, construct: &ImageId) -> Result<ImageKey, ImageError> {
        let target = install_target(request.agent, request.platform)?;
        Ok(ImageKey::compute(request, target, construct))
    }

    pub fn sha256(&self) -> &Sha256 {
        &self.0
    }

    pub(crate) fn compute(
        request: &ImageRequest,
        target: &InstallTarget,
        construct: &ImageId,
    ) -> ImageKey {
        let ImageRequest {
            role,
            context: _,
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
        match revision {
            RoleRevision::Clean { commit } => {
                fields.push("clean");
                fields.push(commit.as_str());
                fields.push("");
            }
            RoleRevision::Dirty { commit, digest } => {
                fields.push("dirty");
                fields.push(commit.as_str());
                fields.push(digest.as_str());
            }
        }
        fields.push(Sha256::of(dockerfile_bytes).as_str());
        fields.push(construct.to_string());
        fields.push(agent.spec().install.version);
        fields.push(target.sha256);
        fields.push(platform.docker_arg());
        fields.push(match docker {
            DockerLayer::Absent => "absent",
            DockerLayer::Client => "client",
        });
        fields.push(Sha256::of(JACKIN_START).as_str());
        fields.push(Sha256::of(DERIVED_DOCKERFILE).as_str());
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
