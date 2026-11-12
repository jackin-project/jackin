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

const AGENT_STAGE_ANCHOR: &str = "FROM construct AS agent\n";
const ARCHIVE_STAGE_ANCHOR: &str = "FROM scratch AS docker-cli-archive-arm64";
const TRUSTED_COPY_ANCHOR: &str = "COPY --from=agent --chown=root:root /out/ /\n";
const ASSEMBLED_ARGS_ANCHOR: &str = "ARG AGENT_EXTRA_BINARY\n\nUSER root\nCOPY --from=layout";
const ASSEMBLED_CONFIG_ANCHOR: &str = "COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${AGENT_SYSTEM_CONFIG} ${AGENT_SYSTEM_CONFIG}\n";
const SCAN_ARGS_ANCHOR: &str = "ARG AGENT_EXTRA_BINARY\n\nUSER root\nCOPY scan /jackin-scan";
const SCAN_SAME_AS_ANCHOR: &str = "        --same-as ${AGENT_BINARY}=/trusted${AGENT_BINARY} \\\n";
const SCAN_FILES_ANCHOR: &str = "        ${AGENT_BINARY} \\\n";
const AGENT_RUN_ANCHOR: &str = "RUN --mount=type=bind,from=agent-archive";

fn agent_run_template(text: &str) -> String {
    let start = text.find(AGENT_RUN_ANCHOR).unwrap_or(0);
    let end = text[start..]
        .find("\n\nFROM ")
        .map_or(text.len(), |at| start + at);
    text[start..end].to_owned()
}

fn archive_stages(number: usize, prefix: &str) -> String {
    format!(
        "FROM scratch AS agent{number}-archive-arm64\nARG {prefix}_SHA256_ARM64\nARG {prefix}_URL_ARM64\nADD --checksum=sha256:${{{prefix}_SHA256_ARM64}} ${{{prefix}_URL_ARM64}} /agent-artifact\n\nFROM scratch AS agent{number}-archive-amd64\nARG {prefix}_SHA256_AMD64\nARG {prefix}_URL_AMD64\nADD --checksum=sha256:${{{prefix}_SHA256_AMD64}} ${{{prefix}_URL_AMD64}} /agent-artifact\n\nFROM agent{number}-archive-${{TARGETARCH}} AS agent{number}-archive\n\n"
    )
}

fn agent_stage(number: usize, prefix: &str, template: &str) -> String {
    let run = template
        .replace("AGENT_", &format!("{prefix}_"))
        .replace("agent-archive", &format!("agent{number}-archive"));
    format!(
        "FROM construct AS agent{number}\nARG {prefix}_VERSION\nARG {prefix}_BIN_DIR\nARG {prefix}_BINARY\nARG {prefix}_ARCHIVE_MEMBERS\nARG {prefix}_VERSION_OUTPUT\nARG {prefix}_SYSTEM_CONFIG\nARG {prefix}_SYSTEM_CONFIG_CONTENT\nARG {prefix}_EXTRA_BINARY\n\nUSER root\n{run}\n\n"
    )
}

fn multi_dockerfile(agents: &[jackin_core::Agent]) -> Vec<u8> {
    let text = String::from_utf8_lossy(DERIVED_DOCKERFILE).into_owned();
    let template = agent_run_template(&text);
    let mut out = text;
    let mut archives = String::new();
    let mut stages = String::new();
    let mut extras: Vec<(usize, String)> = Vec::new();
    for index in 1..agents.len() {
        let number = index + 1;
        let prefix = format!("AGENT{number}");
        archives.push_str(&archive_stages(number, &prefix));
        stages.push_str(&agent_stage(number, &prefix, &template));
        extras.push((number, prefix));
    }
    if extras.is_empty() {
        return out.into_bytes();
    }
    out = out.replacen(
        AGENT_STAGE_ANCHOR,
        &format!("{archives}{AGENT_STAGE_ANCHOR}"),
        1,
    );
    out = out.replacen(
        ARCHIVE_STAGE_ANCHOR,
        &format!("{stages}{ARCHIVE_STAGE_ANCHOR}"),
        1,
    );
    let mut trusted = String::from(TRUSTED_COPY_ANCHOR);
    for (number, _) in &extras {
        trusted.push_str(&format!(
            "COPY --from=agent{number} --chown=root:root /out/ /\n"
        ));
    }
    out = out.replacen(TRUSTED_COPY_ANCHOR, &trusted, 1);
    let mut assembled_args = String::from("ARG AGENT_EXTRA_BINARY\n");
    for (_, prefix) in &extras {
        assembled_args.push_str(&format!("ARG {prefix}_SYSTEM_CONFIG\n"));
    }
    assembled_args.push_str("\nUSER root\nCOPY --from=layout");
    out = out.replacen(ASSEMBLED_ARGS_ANCHOR, &assembled_args, 1);
    let mut assembled_configs = String::from(ASSEMBLED_CONFIG_ANCHOR);
    for (_, prefix) in &extras {
        assembled_configs.push_str(&format!(
            "COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX ${{{prefix}_SYSTEM_CONFIG}} ${{{prefix}_SYSTEM_CONFIG}}\n"
        ));
    }
    out = out.replacen(ASSEMBLED_CONFIG_ANCHOR, &assembled_configs, 1);
    let mut scan_args = String::from("ARG AGENT_EXTRA_BINARY\n");
    for (_, prefix) in &extras {
        scan_args.push_str(&format!(
            "ARG {prefix}_BINARY\nARG {prefix}_SYSTEM_CONFIG\n"
        ));
    }
    scan_args.push_str("\nUSER root\nCOPY scan /jackin-scan");
    out = out.replacen(SCAN_ARGS_ANCHOR, &scan_args, 1);
    let mut scan_same_as = String::from(SCAN_SAME_AS_ANCHOR);
    for (_, prefix) in &extras {
        scan_same_as.push_str(&format!(
            "        --same-as ${{{prefix}_BINARY}}=/trusted${{{prefix}_BINARY}} \\\n"
        ));
        scan_same_as.push_str(&format!(
            "        --same-as ${{{prefix}_SYSTEM_CONFIG}}=/trusted${{{prefix}_SYSTEM_CONFIG}} \\\n"
        ));
        scan_same_as.push_str(&format!("        --data ${{{prefix}_SYSTEM_CONFIG}} \\\n"));
    }
    out = out.replacen(SCAN_SAME_AS_ANCHOR, &scan_same_as, 1);
    let mut scan_files = String::from(SCAN_FILES_ANCHOR);
    for (_, prefix) in &extras {
        scan_files.push_str(&format!("        ${{{prefix}_BINARY}} \\\n"));
    }
    out = out.replacen(SCAN_FILES_ANCHOR, &scan_files, 1);
    out.into_bytes()
}

const CODEX_SCAN_ABSENT: &str = "        --absent /etc/codex/requirements.toml \\\n        --absent /etc/codex/managed_config.toml \\\n        --absent /etc/codex/hooks.json \\\n        --absent /etc/codex/rules \\\n        --absent /etc/codex/agents \\\n        --absent /etc/codex/skills \\\n";

fn scan_absent_dockerfile(dockerfile: Vec<u8>, agents: &[jackin_core::Agent]) -> Vec<u8> {
    let mut paths: Vec<&str> = Vec::new();
    for agent in agents {
        for path in agent.spec().install.scan_absent_paths {
            if !paths.contains(path) {
                paths.push(path);
            }
        }
    }
    let mut replacement = String::new();
    for path in &paths {
        replacement.push_str("        --absent ");
        replacement.push_str(path);
        replacement.push_str(" \\\n");
    }
    String::from_utf8_lossy(&dockerfile)
        .replace(CODEX_SCAN_ABSENT, &replacement)
        .into_bytes()
}

fn strip_system_config(dockerfile: Vec<u8>, prefix: &str) -> Vec<u8> {
    let config = format!("${{{prefix}_SYSTEM_CONFIG}}");
    let content = format!("${{{prefix}_SYSTEM_CONFIG_CONTENT}}");
    let text = String::from_utf8_lossy(&dockerfile).into_owned();
    let text = text.replace(
        &format!("    && install -d -m 0755 -o root -g root \"/out$(dirname \"{config}\")\" \\\n"),
        "",
    );
    let text = text.replace(
        &format!("    && printf '%s' \"{content}\" > \"/out{config}\" \\\n"),
        "",
    );
    let text = text.replace(
        &format!("COPY --from=trusted --chown=root:root --chmod=u=rwX,go=rX {config} {config}\n"),
        "",
    );
    let text = text.replace(
        &format!("        --same-as {config}=/trusted{config} \\\n"),
        "",
    );
    let text = text.replace(&format!("        --data {config} \\\n"), "");
    text.into_bytes()
}

fn derived_context(request: &ImageRequest) -> Vec<MaterializedFile> {
    let agents = request.sorted_agents();
    let dockerfile = if agents.len() > 1 {
        multi_dockerfile(&agents)
    } else {
        DERIVED_DOCKERFILE.to_vec()
    };
    let mut dockerfile = scan_absent_dockerfile(dockerfile, &agents);
    for (index, agent) in agents.iter().enumerate() {
        if agent.spec().install.system_config.as_str().is_empty() {
            let prefix = if index == 0 {
                "AGENT".to_owned()
            } else {
                format!("AGENT{}", index + 1)
            };
            dockerfile = strip_system_config(dockerfile, &prefix);
        }
    }
    vec![
        file("Dockerfile", &dockerfile),
        file("jackin-start", JACKIN_START),
        file("scan", SCAN),
        hook_file(HookKind::Setup, request.hooks.setup.as_ref()),
        hook_file(HookKind::Preflight, request.hooks.preflight.as_ref()),
    ]
}
