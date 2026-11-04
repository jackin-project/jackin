// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use jackin_core::{FileMode, HookKind, ImageId, ImageTag, SandboxPath, Sha256, sandbox_layout};

use crate::assets::{CONSTRUCT_DOCKERFILE, DERIVED_DOCKERFILE, JACKIN_START, SCAN};
use crate::dockerfile::{DockerfileError, validate_dockerfile};
use crate::error::ImageError;
use crate::key::{Fields, ImageKey, derived_arguments};
use crate::request::{ContextEntry, ImageRequest, Rebuild, SourceFile};

const CONTEXT_MODE: FileMode = FileMode(0o644);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildStage {
    Construct,
    Role,
    Derived,
}

impl fmt::Display for BuildStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BuildStage::Construct => "construct",
            BuildStage::Role => "role",
            BuildStage::Derived => "derived",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedBuild {
    pub stage: BuildStage,
    pub tag: ImageTag,
    pub args: Vec<String>,
    pub context: ContextSource,
    pub stdin: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextSource {
    Entries(Vec<ContextEntry>),
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
    construct: PlannedBuild,
    derived_args: Vec<String>,
}

impl ImagePlan {
    pub fn new(request: &ImageRequest) -> Result<ImagePlan, ImageError> {
        let text =
            std::str::from_utf8(&request.dockerfile.bytes).map_err(|_| DockerfileError::NotUtf8)?;
        validate_dockerfile(text)?;
        Ok(ImagePlan {
            request: request.clone(),
            construct: construct_build(request),
            derived_args: derived_arguments(request.agent, request.docker)?,
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
        role_args.push("--file=-".to_owned());
        role_args.push(format!(
            "--build-context=construct=docker-image://{}",
            self.construct.tag
        ));
        let mut derived_args = common_arguments(request, &derived_tag);
        derived_args.push(format!("--build-context=role=docker-image://{role_tag}"));
        derived_args.push(format!(
            "--build-context=construct=docker-image://{}",
            self.construct.tag
        ));
        derived_args.extend(self.derived_args.iter().cloned());
        FinalBuilds {
            key,
            role: PlannedBuild {
                stage: BuildStage::Role,
                tag: role_tag,
                args: role_args,
                context: entries_context(request),
                stdin: Some(request.dockerfile.bytes.clone()),
            },
            derived: PlannedBuild {
                stage: BuildStage::Derived,
                tag: derived_tag,
                args: derived_args,
                context: ContextSource::Materialized(derived_context(request)),
                stdin: None,
            },
        }
    }

    fn key(&self, construct: &ImageId) -> ImageKey {
        ImageKey::compute(&self.request, &self.derived_args, construct)
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
        stdin: None,
    }
}

fn last_segment(path: &SandboxPath) -> &str {
    path.as_str()
        .rsplit_once('/')
        .map_or(path.as_str(), |(_, name)| name)
}

fn entries_context(request: &ImageRequest) -> ContextSource {
    let mut entries = request.context_files.clone();
    entries.sort_by(|a, b| a.path().cmp(b.path()));
    entries.dedup_by(|a, b| a.path() == b.path());
    ContextSource::Entries(entries)
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
        file("scan", SCAN),
        hook_file(HookKind::Setup, request.hooks.setup.as_ref()),
        hook_file(HookKind::Preflight, request.hooks.preflight.as_ref()),
    ]
}
