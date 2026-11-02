// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Agent, FileMode, HookKind, HostPath, ImageId, ImageTag, InstallTarget, Platform, SandboxPath,
    Sha256, sandbox_layout,
};

use crate::assets::{CONSTRUCT_DOCKERFILE, DERIVED_DOCKERFILE, JACKIN_START};
use crate::dockerfile::{DockerfileError, validate_dockerfile};
use crate::error::ImageError;
use crate::key::{Fields, ImageKey, install_target};
use crate::request::{ImageRequest, Rebuild, SourceFile};

const CONTEXT_MODE: FileMode = FileMode(0o644);
const PLATFORMS: [Platform; 2] = [Platform::LinuxArm64, Platform::LinuxAmd64];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildStage {
    Construct,
    Role,
    Derived,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedBuild {
    pub stage: BuildStage,
    pub tag: ImageTag,
    pub args: Vec<String>,
    pub context: ContextSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextSource {
    RoleRepository(HostPath),
    Materialized(Vec<MaterializedFile>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub mode: FileMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalBuilds {
    pub key: ImageKey,
    pub role: PlannedBuild,
    pub derived: PlannedBuild,
}

#[derive(Clone, Debug)]
pub struct ImagePlan {
    request: ImageRequest,
    target: &'static InstallTarget,
    construct: PlannedBuild,
    derived_args: Vec<String>,
}

impl ImagePlan {
    pub fn new(request: &ImageRequest) -> Result<ImagePlan, ImageError> {
        let text =
            std::str::from_utf8(&request.dockerfile.bytes).map_err(|_| DockerfileError::NotUtf8)?;
        validate_dockerfile(text)?;
        let target = install_target(request.agent, request.platform)?;
        Ok(ImagePlan {
            request: request.clone(),
            target,
            construct: construct_build(request),
            derived_args: derived_arguments(request.agent)?,
        })
    }

    pub fn construct_build(&self) -> &PlannedBuild {
        &self.construct
    }

    pub fn final_tag(&self, construct: &ImageId) -> ImageTag {
        ImageTag::for_role(&self.request.role, self.key(construct).sha256())
    }

    pub fn final_builds(&self, construct: &ImageId) -> FinalBuilds {
        let request = &self.request;
        let key = self.key(construct);
        let role_tag = ImageTag::for_role_stage(&request.role, key.sha256());
        let derived_tag = ImageTag::for_role(&request.role, key.sha256());
        let mut role_args = common_arguments(request, &role_tag);
        role_args.push(format!(
            "--file={}",
            request
                .context
                .as_path()
                .join(request.dockerfile.path.as_str())
                .display()
        ));
        role_args.push(format!(
            "--build-context=construct=docker-image://{}",
            self.construct.tag
        ));
        let mut derived_args = common_arguments(request, &derived_tag);
        derived_args.push(format!("--build-context=role=docker-image://{role_tag}"));
        derived_args.extend(self.derived_args.iter().cloned());
        FinalBuilds {
            key,
            role: PlannedBuild {
                stage: BuildStage::Role,
                tag: role_tag,
                args: role_args,
                context: ContextSource::RoleRepository(request.context.clone()),
            },
            derived: PlannedBuild {
                stage: BuildStage::Derived,
                tag: derived_tag,
                args: derived_args,
                context: ContextSource::Materialized(derived_context(request)),
            },
        }
    }

    fn key(&self, construct: &ImageId) -> ImageKey {
        ImageKey::compute(&self.request, self.target, construct)
    }
}

fn file(name: &str, bytes: &[u8]) -> MaterializedFile {
    MaterializedFile {
        name: name.to_owned(),
        bytes: bytes.to_vec(),
        mode: CONTEXT_MODE,
    }
}

fn common_arguments(request: &ImageRequest, tag: &ImageTag) -> Vec<String> {
    let mut args = vec![
        format!("--builder={}", request.builder),
        format!("--platform={}", request.platform.docker_arg()),
        "--provenance=false".to_owned(),
        "--sbom=false".to_owned(),
        "--load".to_owned(),
        "--progress=plain".to_owned(),
        format!("--tag={tag}"),
    ];
    if request.rebuild == Rebuild::Yes {
        args.push("--no-cache".to_owned());
    }
    args
}

fn construct_build(request: &ImageRequest) -> PlannedBuild {
    let mut fields = Fields::default();
    fields.push(Sha256::of(CONSTRUCT_DOCKERFILE).as_str());
    fields.push(request.platform.docker_arg());
    let tag = ImageTag::for_construct(&fields.digest());
    PlannedBuild {
        stage: BuildStage::Construct,
        args: common_arguments(request, &tag),
        tag,
        context: ContextSource::Materialized(vec![file("Dockerfile", CONSTRUCT_DOCKERFILE)]),
    }
}

fn last_segment(path: &SandboxPath) -> &str {
    path.as_str()
        .rsplit_once('/')
        .map_or(path.as_str(), |(_, name)| name)
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

fn derived_arguments(agent: Agent) -> Result<Vec<String>, ImageError> {
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
    Ok(args)
}

fn hook_file(kind: HookKind, source: Option<&SourceFile>) -> MaterializedFile {
    let name = format!("hooks/{}", last_segment(&sandbox_layout::hook_path(kind)));
    file(
        &name,
        source.map_or(&[][..], |source| source.bytes.as_slice()),
    )
}

fn derived_context(request: &ImageRequest) -> Vec<MaterializedFile> {
    vec![
        file("Dockerfile", DERIVED_DOCKERFILE),
        file("jackin-start", JACKIN_START),
        hook_file(HookKind::Setup, request.hooks.setup.as_ref()),
        hook_file(HookKind::Preflight, request.hooks.preflight.as_ref()),
    ]
}
