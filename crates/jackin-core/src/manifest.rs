// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;

use thiserror::Error;

use crate::boundary::{FileMode, FileSystem, FsError};
use crate::display::plain;
use crate::fs::write_atomic;
use crate::ids::{EnvName, IdError, ModelName, RoleName};
use crate::paths::{HostPath, RepoPath};
use crate::tomlkit::{Finding, Reader, parse_table, quote};

const MANIFEST_MODE: FileMode = FileMode(0o644);
const ENV_TEXT: &str = "expected text without a newline or NUL";
const REPO_PATH: &str = "expected a relative path without empty, . or .. segments";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleManifest {
    pub schema: SchemaVersion,
    pub role: RoleSection,
    pub codex: CodexSection,
    pub docker: DockerSection,
    pub hooks: HooksSection,
    pub env: BTreeMap<EnvName, EnvSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaVersion;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleSection {
    pub name: RoleName,
    pub dockerfile: RepoPath,
    pub policy: RepoPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexSection {
    pub model: Option<ModelName>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockerSection {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HooksSection {
    pub setup: Option<RepoPath>,
    pub preflight: Option<RepoPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvSpec {
    pub default: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HookKind {
    Setup,
    Preflight,
}

impl fmt::Display for HookKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HookKind::Setup => "setup",
            HookKind::Preflight => "preflight",
        })
    }
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("syntax error at line {line}, column {column}: {}", plain(.message))]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("unknown field {}", plain(.key))]
    UnknownField { key: String },
    #[error("missing field {}", plain(.key))]
    MissingField { key: String },
    #[error("invalid value for {}: {}", plain(.key), plain(.message))]
    InvalidValue { key: String, message: String },
    #[error("unsupported schema version {found}")]
    UnsupportedSchema { found: i64 },
    #[error("reserved environment name {0}")]
    ReservedEnvName(EnvName),
    #[error(transparent)]
    Fs(#[from] FsError),
}

impl From<Finding> for ManifestError {
    fn from(finding: Finding) -> ManifestError {
        match finding {
            Finding::Unknown { key } => ManifestError::UnknownField { key },
            Finding::Missing { key } => ManifestError::MissingField { key },
            Finding::Invalid { key, message } => ManifestError::InvalidValue { key, message },
        }
    }
}

fn id_message(error: IdError) -> String {
    error.to_string()
}

fn repo_path(text: &str) -> Result<RepoPath, &'static str> {
    RepoPath::parse(text).map_err(|_error| REPO_PATH)
}

fn env_text(text: &str) -> Result<String, &'static str> {
    if text.contains(['\n', '\0']) {
        Err(ENV_TEXT)
    } else {
        Ok(text.to_owned())
    }
}

fn static_message(message: &'static str) -> String {
    message.to_owned()
}

fn read_schema(root: &mut Reader<'_, '_>, errors: &mut Vec<ManifestError>) {
    if !root.contains("schema") {
        root.report_missing("schema");
        return;
    }
    match root.integer("schema") {
        Some(1) | None => {}
        Some(found) => errors.push(ManifestError::UnsupportedSchema { found }),
    }
}

fn read_role(root: &mut Reader<'_, '_>) -> Option<RoleSection> {
    let mut role = root.required_table("role")?;
    let name = role.required_parsed("name", RoleName::parse, id_message);
    let dockerfile = role.required_parsed("dockerfile", repo_path, static_message);
    let policy = role.required_parsed("policy", repo_path, static_message);
    role.finish();
    Some(RoleSection {
        name: name?,
        dockerfile: dockerfile?,
        policy: policy?,
    })
}

fn read_codex(root: &mut Reader<'_, '_>) -> CodexSection {
    let mut model = None;
    if let Some(mut codex) = root.table("codex") {
        model = codex.parsed("model", ModelName::parse, id_message);
        codex.finish();
    }
    CodexSection { model }
}

fn read_docker(root: &mut Reader<'_, '_>) -> DockerSection {
    let mut enabled = false;
    if let Some(mut docker) = root.table("docker") {
        enabled = docker.boolean("enabled").unwrap_or(false);
        docker.finish();
    }
    DockerSection { enabled }
}

fn read_hooks(root: &mut Reader<'_, '_>) -> HooksSection {
    let mut hooks = HooksSection {
        setup: None,
        preflight: None,
    };
    if let Some(mut section) = root.table("hooks") {
        hooks.setup = section.parsed("setup", repo_path, static_message);
        hooks.preflight = section.parsed("preflight", repo_path, static_message);
        section.finish();
    }
    hooks
}

fn read_env(
    root: &mut Reader<'_, '_>,
    reserved: &mut Vec<ManifestError>,
) -> BTreeMap<EnvName, EnvSpec> {
    let mut out = BTreeMap::new();
    let Some(mut env) = root.table("env") else {
        return out;
    };
    for (key, value) in env.entries() {
        let name = match EnvName::parse(key) {
            Ok(name) => name,
            Err(error) => {
                env.report_invalid(key, &error.to_string());
                continue;
            }
        };
        let Some(mut entry) = env.entry_table(key, value) else {
            continue;
        };
        let default = entry.parsed("default", env_text, static_message);
        let prompt = entry.parsed("prompt", env_text, static_message);
        entry.finish();
        if name.is_reserved() {
            reserved.push(ManifestError::ReservedEnvName(name));
        } else {
            out.insert(name, EnvSpec { default, prompt });
        }
    }
    out
}

struct Findings {
    first: ManifestError,
    rest: Vec<ManifestError>,
}

fn analyze(text: &str) -> Result<RoleManifest, Findings> {
    let table = match parse_table(text) {
        Ok(table) => table,
        Err(problem) => {
            return Err(Findings {
                first: ManifestError::Syntax {
                    line: problem.line,
                    column: problem.column,
                    message: problem.message,
                },
                rest: Vec::new(),
            });
        }
    };
    let mut sink = Vec::new();
    let mut errors = Vec::new();
    let mut reserved = Vec::new();
    let mut root = Reader::new(String::new(), &table, &mut sink);
    read_schema(&mut root, &mut errors);
    let role = read_role(&mut root);
    let codex = read_codex(&mut root);
    let docker = read_docker(&mut root);
    let hooks = read_hooks(&mut root);
    let env = read_env(&mut root, &mut reserved);
    root.finish();
    errors.extend(sink.into_iter().map(ManifestError::from));
    errors.extend(reserved);
    let mut all = errors.into_iter();
    match (all.next(), role) {
        (None, Some(role)) => Ok(RoleManifest {
            schema: SchemaVersion,
            role,
            codex,
            docker,
            hooks,
            env,
        }),
        (Some(first), _) => Err(Findings {
            first,
            rest: all.collect(),
        }),
        (None, None) => Err(Findings {
            first: ManifestError::MissingField {
                key: "role".to_owned(),
            },
            rest: Vec::new(),
        }),
    }
}

impl RoleManifest {
    pub fn parse(text: &str) -> Result<RoleManifest, ManifestError> {
        analyze(text).map_err(|findings| findings.first)
    }

    pub fn findings(text: &str) -> Vec<ManifestError> {
        match analyze(text) {
            Ok(_manifest) => Vec::new(),
            Err(findings) => std::iter::once(findings.first)
                .chain(findings.rest)
                .collect(),
        }
    }

    pub fn hook(&self, kind: HookKind) -> Option<&RepoPath> {
        match kind {
            HookKind::Setup => self.hooks.setup.as_ref(),
            HookKind::Preflight => self.hooks.preflight.as_ref(),
        }
    }

    pub fn render(&self) -> String {
        let mut blocks = vec![
            "schema = 1\n".to_owned(),
            format!(
                "[role]\nname = {}\ndockerfile = {}\npolicy = {}\n",
                quote(self.role.name.as_str()),
                quote(self.role.dockerfile.as_str()),
                quote(self.role.policy.as_str()),
            ),
        ];
        if let Some(model) = &self.codex.model {
            blocks.push(format!("[codex]\nmodel = {}\n", quote(model.as_str())));
        }
        blocks.push(format!("[docker]\nenabled = {}\n", self.docker.enabled));
        let hooks: Vec<String> = [
            ("setup", &self.hooks.setup),
            ("preflight", &self.hooks.preflight),
        ]
        .into_iter()
        .filter_map(|(name, path)| {
            path.as_ref()
                .map(|path| format!("{name} = {}\n", quote(path.as_str())))
        })
        .collect();
        if !hooks.is_empty() {
            blocks.push(format!("[hooks]\n{}", hooks.concat()));
        }
        for (name, spec) in &self.env {
            let mut block = format!("[env.{name}]\n");
            if let Some(default) = &spec.default {
                block.push_str(&format!("default = {}\n", quote(default)));
            }
            if let Some(prompt) = &spec.prompt {
                block.push_str(&format!("prompt = {}\n", quote(prompt)));
            }
            blocks.push(block);
        }
        blocks.join("\n")
    }

    pub async fn write(&self, fs: &dyn FileSystem, path: &HostPath) -> Result<(), ManifestError> {
        write_atomic(fs, path, self.render().as_bytes(), MANIFEST_MODE).await?;
        Ok(())
    }
}
