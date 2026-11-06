// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use jackin_core::{FileMode, HookKind, ImageId, ImageTag, SandboxPath, Sha256, sandbox_layout};

use crate::assets::{CONSTRUCT_DOCKERFILE, DERIVED_DOCKERFILE, JACKIN_START, SCAN};
use crate::dockerfile::{DockerfileError, validate_dockerfile};
use crate::error::ImageError;
use crate::key::{Fields, ImageKey, derived_arguments_multi};
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
            derived_args: derived_arguments_multi(&request.sorted_agents(), request.docker)?,
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

fn multi_dockerfile(agents: &[jackin_core::Agent]) -> Vec<u8> {
    let text = String::from_utf8_lossy(DERIVED_DOCKERFILE).into_owned();
    let mut out = text;
    for index in 1..agents.len() {
        let n = index + 1;
        let prefix = format!("AGENT{n}");
        let archive = format!(
            "FROM scratch AS agent{n}-archive-arm64\nARG {prefix}_SHA256_ARM64\nARG {prefix}_URL_ARM64\nADD --checksum=sha256:${{{prefix}_SHA256_ARM64}} ${{{prefix}_URL_ARM64}} /agent-artifact\n\nFROM scratch AS agent{n}-archive-amd64\nARG {prefix}_SHA256_AMD64\nARG {prefix}_URL_AMD64\nADD --checksum=sha256:${{{prefix}_SHA256_AMD64}} ${{{prefix}_URL_AMD64}} /agent-artifact\n\nFROM agent{n}-archive-${{TARGETARCH}} AS agent{n}-archive\n\n"
        );
        out = out.replacen(
            "FROM construct AS agent\n",
            &format!("{archive}FROM construct AS agent\n"),
            1,
        );
        let start = out
            .find("RUN --mount=type=bind,from=agent-archive")
            .unwrap_or(0);
        let end = out[start..]
            .find("\n\nFROM scratch AS docker-cli-archive-arm64")
            .map_or(out.len(), |at| start + at);
        let run = out[start..end]
            .replace("AGENT_", &format!("{prefix}_"))
            .replace("agent-archive", &format!("agent{n}-archive"));
        let stage = format!(
            "FROM construct AS agent{n}\nARG {prefix}_VERSION\nARG {prefix}_BIN_DIR\nARG {prefix}_BINARY\nARG {prefix}_ARCHIVE_MEMBERS\nARG {prefix}_VERSION_OUTPUT\nARG {prefix}_SYSTEM_CONFIG\nARG {prefix}_SYSTEM_CONFIG_CONTENT\nARG {prefix}_EXTRA_BINARY\n\nUSER root\n{run}\n\n"
        );
        out = out.replacen(
            "FROM scratch AS docker-cli-archive-arm64",
            &format!("{stage}FROM scratch AS docker-cli-archive-arm64"),
            1,
        );
        out = out.replacen(
            "COPY --from=agent --chown=root:root /out/ /\n",
            &format!(
                "COPY --from=agent --chown=root:root /out/ /\nCOPY --from=agent{n} --chown=root:root /out/ /\n"
            ),
            1,
        );
        out = out.replacen(
            "ARG AGENT_EXTRA_BINARY\n\nUSER root\nCOPY --from=layout",
            &format!(
                "ARG AGENT_EXTRA_BINARY\nARG {prefix}_SYSTEM_CONFIG\n\nUSER root\nCOPY --from=layout"
            ),
            1,
        );
        out = out.replacen(
            "COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${AGENT_SYSTEM_CONFIG} ${AGENT_SYSTEM_CONFIG}\n",
            &format!(
                "COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${{AGENT_SYSTEM_CONFIG}} ${{AGENT_SYSTEM_CONFIG}}\nCOPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${{{prefix}_SYSTEM_CONFIG}} ${{{prefix}_SYSTEM_CONFIG}}\n"
            ),
            1,
        );
        out = out.replacen(
            "ARG AGENT_EXTRA_BINARY\n\nUSER root\nCOPY scan /jackin-scan",
            &format!(
                "ARG AGENT_EXTRA_BINARY\nARG {prefix}_BINARY\nARG {prefix}_SYSTEM_CONFIG\n\nUSER root\nCOPY scan /jackin-scan"
            ),
            1,
        );
        out = out.replacen(
            "--same-as ${AGENT_BINARY}=/trusted${AGENT_BINARY}",
            &format!(
                "--same-as ${{AGENT_BINARY}}=/trusted${{AGENT_BINARY}} \\\n        --same-as ${{{prefix}_BINARY}}=/trusted${{{prefix}_BINARY}} \\\n        --same-as ${{{prefix}_SYSTEM_CONFIG}}=/trusted${{{prefix}_SYSTEM_CONFIG}} \\\n        --data ${{{prefix}_SYSTEM_CONFIG}}"
            ),
            1,
        );
        out = out.replacen(
            "        ${AGENT_BINARY} \\",
            &format!("        ${{AGENT_BINARY}} \\\n        ${{{prefix}_BINARY}} \\"),
            1,
        );
    }
    out.into_bytes()
}

fn derived_context(request: &ImageRequest) -> Vec<MaterializedFile> {
    let agents = request.sorted_agents();
    let dockerfile = if agents.len() > 1 {
        multi_dockerfile(&agents)
    } else {
        DERIVED_DOCKERFILE.to_vec()
    };
    vec![
        file("Dockerfile", &dockerfile),
        file("jackin-start", JACKIN_START),
        file("scan", SCAN),
        hook_file(HookKind::Setup, request.hooks.setup.as_ref()),
        hook_file(HookKind::Preflight, request.hooks.preflight.as_ref()),
    ]
}
