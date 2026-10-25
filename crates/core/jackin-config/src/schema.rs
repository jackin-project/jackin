// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{Agent, EnvValue, JackinPaths, MountIsolation};
use jackin_core::{DockerGrants, DockerSecurityProfile};
use serde::{Deserialize, Serialize};

use crate::ConfigError;
use crate::accounts::AgentConfiguration;
use crate::auth::GithubAuthConfig;
use crate::versions::current_workspace_version;

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) const fn bool_matches<const EXPECTED: bool>(v: &bool) -> bool {
    *v == EXPECTED
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DirtyExitPolicy {
    #[default]
    Ask,
    Keep,
    Discard,
}

impl DirtyExitPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Keep => "keep",
            Self::Discard => "discard",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub isolation: MountIsolation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeepAwakeConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl KeepAwakeConfig {
    pub const fn is_default(&self) -> bool {
        !self.enabled
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRoleOverride {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_bindings: BTreeMap<Agent, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_launch: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
}

impl WorkspaceRoleOverride {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_backend: Option<String>,
}

impl RuntimeConfig {
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.default_backend.is_none()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryLevelConfig {
    Info,
    Debug,
    Trace,
}

impl TelemetryLevelConfig {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<TelemetryLevelConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

impl TelemetryConfig {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.level.is_none() && self.categories.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceRuntimeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
}

impl WorkspaceRuntimeConfig {
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.backend.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_bindings: BTreeMap<Agent, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_launch: Option<Vec<String>>,
    #[serde(default = "current_workspace_version", rename = "version")]
    pub version: String,
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_roles: Vec<String>,
    #[serde(default)]
    pub default_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<Agent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_role: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, WorkspaceRoleOverride>,
    #[serde(default, skip_serializing_if = "KeepAwakeConfig::is_default")]
    pub keep_awake: KeepAwakeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
    #[serde(default, skip_serializing_if = "bool_matches::<false>")]
    pub git_pull_on_entry: bool,
    #[serde(default, skip_serializing_if = "WorkspaceRuntimeConfig::is_default")]
    pub runtime: WorkspaceRuntimeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<DirtyExitPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<WorkspaceDockerConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDockerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DockerSecurityProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants: Option<DockerGrants>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            version: current_workspace_version(),
            accounts: Vec::new(),
            account_bindings: BTreeMap::new(),
            default_launch: None,
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_roles: Vec::new(),
            default_role: None,
            default_agent: None,
            last_role: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            keep_awake: KeepAwakeConfig::default(),
            github: None,
            git_pull_on_entry: false,
            runtime: WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
        }
    }
}

impl WorkspaceConfig {
    pub fn resolved_agent(&self) -> Agent {
        self.default_agent.unwrap_or(Agent::Claude)
    }

    #[must_use]
    pub fn validate_default_launch_list(
        ids: &[String],
        allowlist: Option<&[String]>,
        configurations: &BTreeMap<String, AgentConfiguration>,
    ) -> Vec<String> {
        use std::collections::BTreeSet;
        let mut errors = Vec::new();
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                errors.push(format!("duplicate launch configuration {id:?}"));
                continue;
            }
            let Some(config) = configurations.get(id) else {
                errors.push(format!("unknown agent configuration {id:?}"));
                continue;
            };
            if allowlist.is_some_and(|ids| !ids.contains(&config.account)) {
                errors.push(format!(
                    "account {:?} is not assigned to this workspace",
                    config.account
                ));
            }
        }
        errors
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSource {
    pub git: String,
    #[serde(default, skip_serializing_if = "bool_matches::<false>")]
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalMountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}

impl From<GlobalMountConfig> for MountConfig {
    fn from(g: GlobalMountConfig) -> Self {
        Self {
            src: g.src,
            dst: g.dst,
            readonly: g.readonly,
            isolation: MountIsolation::Shared,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    Mount(GlobalMountConfig),
    Scoped(BTreeMap<String, GlobalMountConfig>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(BTreeMap<String, MountEntry>);

impl DockerMounts {
    pub fn get(&self, key: &str) -> Option<&MountEntry> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: String, value: MountEntry) -> Option<MountEntry> {
        self.0.insert(key, value)
    }

    pub fn entry(
        &mut self,
        key: String,
    ) -> std::collections::btree_map::Entry<'_, String, MountEntry> {
        self.0.entry(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &MountEntry)> {
        self.0.iter()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DockerSecurityProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants: Option<DockerGrants>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    pub name: String,
    pub label: String,
    pub workdir: String,
    pub mounts: Vec<MountConfig>,
    pub keep_awake_enabled: bool,
    pub default_agent: Option<Agent>,
    pub git_pull_on_entry: bool,
    pub mount_heal: MountHealReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealedMountSource {
    pub name: Option<String>,
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountHealReport {
    pub recreated: Vec<HealedMountSource>,
    pub skipped: Vec<HealedMountSource>,
}

impl MountHealReport {
    pub fn is_empty(&self) -> bool {
        self.recreated.is_empty() && self.skipped.is_empty()
    }

    pub fn merge(&mut self, other: Self) {
        self.recreated.extend(other.recreated);
        self.skipped.extend(other.skipped);
    }

    pub fn notice_lines(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.recreated.len() + self.skipped.len());
        for healed in &self.recreated {
            match &healed.name {
                Some(name) => lines.push(format!(
                    "recreated missing cache directory for global mount {name:?}: {} -> {}",
                    healed.src, healed.dst
                )),
                None => lines.push(format!(
                    "recreated missing cache directory: {} -> {}",
                    healed.src, healed.dst
                )),
            }
        }
        for healed in &self.skipped {
            match &healed.name {
                Some(name) => lines.push(format!(
                    "warning: skipping global mount {name:?}: source {} does not exist \
                     (container path {} will be unavailable); restore the file or remove \
                     the mount with: jackin config mount remove {name:?}",
                    healed.src, healed.dst
                )),
                None => lines.push(format!(
                    "warning: skipping workspace mount: source {} does not exist \
                     (container path {} will be unavailable); restore the file or remove \
                     the mount from the workspace",
                    healed.src, healed.dst
                )),
            }
        }
        lines
    }
}

impl ResolvedWorkspace {
    pub fn as_workspace_label(
        &self,
    ) -> Result<jackin_core::WorkspaceLabel, jackin_core::WorkspaceLabelError> {
        jackin_core::WorkspaceLabel::parse(&self.label)
    }
}

pub fn validate_mount_specs(mounts: &[MountConfig]) -> crate::ConfigResult<()> {
    use std::collections::HashSet;
    use std::path::Path;
    let mut seen_dst = HashSet::new();
    for mount in mounts {
        if !Path::new(&mount.src).is_absolute() {
            return Err(ConfigError::MountSrcNotAbsolute(mount.src.clone()));
        }
        if !mount.dst.starts_with('/') {
            return Err(ConfigError::MountDstNotAbsolute(mount.dst.clone()));
        }
        if has_dot_component(&mount.src) {
            return Err(ConfigError::msg(format_args!(
                "mount source must not contain `.` or `..` components: {}",
                mount.src
            )));
        }
        if has_dot_component(&mount.dst) {
            return Err(ConfigError::msg(format_args!(
                "mount destination must not contain `.` or `..` components: {}",
                mount.dst
            )));
        }
        if !seen_dst.insert(mount.dst.clone()) {
            return Err(ConfigError::DuplicateMountDst(mount.dst.clone()));
        }
    }
    Ok(())
}

fn has_dot_component(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|component| matches!(component, "." | ".."))
}

pub fn validate_mount_paths(mounts: &[MountConfig]) -> crate::ConfigResult<()> {
    use std::path::Path;
    for mount in mounts {
        if !Path::new(&mount.src).exists() {
            return Err(ConfigError::MountSrcMissing(mount.src.clone()));
        }
    }
    Ok(())
}

pub fn validate_mounts(mounts: &[MountConfig]) -> crate::ConfigResult<()> {
    validate_mount_specs(mounts)?;
    validate_mount_paths(mounts)
}

pub fn launch_cache_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Some(base) = directories::BaseDirs::new() {
        roots.push(base.cache_dir().to_path_buf());
        roots.push(base.home_dir().join(".cache"));
    }
    if let Ok(paths) = JackinPaths::detect() {
        roots.push(paths.cache_dir);
    }
    roots.sort();
    roots.dedup();
    roots
}

pub fn ensure_mount_sources(
    mounts: Vec<(Option<String>, MountConfig)>,
    cache_roots: &[std::path::PathBuf],
) -> (Vec<MountConfig>, MountHealReport) {
    let mut kept = Vec::with_capacity(mounts.len());
    let mut report = MountHealReport::default();
    for (name, mount) in mounts {
        if std::path::Path::new(&mount.src).exists() {
            kept.push(mount);
            continue;
        }
        let under_cache = cache_roots
            .iter()
            .any(|root| std::path::Path::new(&mount.src).starts_with(root));
        if !under_cache {
            kept.push(mount);
            continue;
        }
        let file_like = std::path::Path::new(&mount.src)
            .extension()
            .is_some_and(|extension| !extension.is_empty());
        if file_like {
            report.skipped.push(HealedMountSource {
                name,
                src: mount.src,
                dst: mount.dst,
            });
            continue;
        }
        if std::fs::create_dir_all(&mount.src).is_ok() {
            report.recreated.push(HealedMountSource {
                name,
                src: mount.src.clone(),
                dst: mount.dst.clone(),
            });
            kept.push(mount);
        } else {
            kept.push(mount);
        }
    }
    (kept, report)
}

#[derive(Debug, Default, Clone)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub no_workdir_mount: bool,
    pub allowed_roles_to_add: Vec<String>,
    pub allowed_roles_to_remove: Vec<String>,
    pub default_role: Option<Option<String>>,
    pub default_agent: Option<Option<Agent>>,
    pub mount_isolation_overrides: Vec<(String, MountIsolation)>,
    pub keep_awake_enabled: Option<bool>,
    pub git_pull_on_entry_enabled: Option<bool>,
    pub default_launch: Option<Option<Vec<String>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    #[serde(default, skip_serializing_if = "bool_matches::<false>")]
    pub coauthor_trailer: bool,
    #[serde(default, skip_serializing_if = "bool_matches::<false>")]
    pub dco: bool,
}

impl GitConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
