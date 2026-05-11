use crate::agent::Agent;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const INSTANCE_MANIFEST_VERSION: u32 = 1;
pub const INSTANCE_INDEX_VERSION: u32 = 1;
const INSTANCE_INDEX_FILE: &str = "instances.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Active,
    Running,
    CleanExited,
    Crashed,
    PreservedDirty,
    PreservedUnpushed,
    RestoreAvailable,
    Superseded,
    Purged,
    FailedSetup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerResources {
    pub role_container: String,
    pub dind_container: String,
    pub network: String,
    pub certs_volume: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceManifest {
    pub version: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub legacy_name: bool,
    pub instance_id: String,
    pub container_base: String,
    pub created_at: String,
    pub updated_at: String,
    pub workspace_name: Option<String>,
    pub workspace_label: String,
    pub workdir: String,
    pub host_workdir_fingerprint: String,
    pub role_key: String,
    pub role_display_name: String,
    pub agent_runtime: String,
    pub role_source_git: String,
    pub role_source_ref: Option<String>,
    pub image_tag: String,
    pub status: InstanceStatus,
    pub last_attach_outcome: Option<String>,
    pub docker: DockerResources,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndexEntry {
    pub instance_id: String,
    pub container_base: String,
    pub workspace_name: Option<String>,
    pub workspace_label: String,
    pub workdir: String,
    pub role_key: String,
    pub agent_runtime: String,
    pub status: InstanceStatus,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndex {
    pub version: u32,
    pub instances: Vec<InstanceIndexEntry>,
}

pub struct NewInstanceManifest<'a> {
    pub container_base: &'a str,
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub host_workdir_fingerprint: &'a str,
    pub role_key: &'a str,
    pub role_display_name: &'a str,
    pub agent_runtime: Agent,
    pub role_source_git: &'a str,
    pub role_source_ref: Option<&'a str>,
    pub image_tag: &'a str,
    pub docker: DockerResources,
}

#[derive(Debug, Clone, Copy)]
pub struct InstanceQuery<'a> {
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub role_key: Option<&'a str>,
    pub agent_runtime: Option<Agent>,
}

impl InstanceManifest {
    pub fn new(input: NewInstanceManifest<'_>) -> Self {
        let now = now_rfc3339();
        Self {
            version: INSTANCE_MANIFEST_VERSION,
            legacy_name: false,
            instance_id: input
                .container_base
                .rsplit_once('-')
                .map_or(input.container_base, |(_, id)| id)
                .to_string(),
            container_base: input.container_base.to_string(),
            created_at: now.clone(),
            updated_at: now,
            workspace_name: input.workspace_name.map(ToOwned::to_owned),
            workspace_label: input.workspace_label.to_string(),
            workdir: input.workdir.to_string(),
            host_workdir_fingerprint: input.host_workdir_fingerprint.to_string(),
            role_key: input.role_key.to_string(),
            role_display_name: input.role_display_name.to_string(),
            agent_runtime: input.agent_runtime.slug().to_string(),
            role_source_git: input.role_source_git.to_string(),
            role_source_ref: input.role_source_ref.map(ToOwned::to_owned),
            image_tag: input.image_tag.to_string(),
            status: InstanceStatus::Active,
            last_attach_outcome: None,
            docker: input.docker,
        }
    }

    pub fn mark_status(&mut self, status: InstanceStatus) {
        self.status = status;
        self.updated_at = now_rfc3339();
    }

    pub const fn is_restore_candidate(&self) -> bool {
        matches!(
            self.status,
            InstanceStatus::Active
                | InstanceStatus::Running
                | InstanceStatus::Crashed
                | InstanceStatus::PreservedDirty
                | InstanceStatus::PreservedUnpushed
                | InstanceStatus::RestoreAvailable
                | InstanceStatus::FailedSetup
        )
    }

    pub fn read(state_dir: &Path) -> anyhow::Result<Self> {
        let path = state_dir.join(".jackin/instance.json");
        let bytes = std::fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn write(&self, state_dir: &Path) -> anyhow::Result<()> {
        let manifest_dir = state_dir.join(".jackin");
        std::fs::create_dir_all(&manifest_dir)?;
        let path = manifest_dir.join("instance.json");
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

pub fn host_path_fingerprint(path: &str) -> String {
    let canonical = std::fs::canonicalize(path)
        .ok()
        .map_or_else(|| path.to_string(), |path| path.display().to_string());
    let digest = Sha256::digest(canonical.as_bytes());
    let hex: String = digest
        .iter()
        .flat_map(|byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            [
                HEX[(byte >> 4) as usize] as char,
                HEX[(byte & 0x0f) as usize] as char,
            ]
        })
        .collect();
    format!("sha256:{hex}")
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !value
}

impl InstanceIndexEntry {
    fn from_manifest(manifest: &InstanceManifest) -> Self {
        Self {
            instance_id: manifest.instance_id.clone(),
            container_base: manifest.container_base.clone(),
            workspace_name: manifest.workspace_name.clone(),
            workspace_label: manifest.workspace_label.clone(),
            workdir: manifest.workdir.clone(),
            role_key: manifest.role_key.clone(),
            agent_runtime: manifest.agent_runtime.clone(),
            status: manifest.status,
            updated_at: manifest.updated_at.clone(),
        }
    }

    fn matches(&self, query: InstanceQuery<'_>) -> bool {
        self.workspace_name.as_deref() == query.workspace_name
            && self.workspace_label == query.workspace_label
            && self.workdir == query.workdir
            && query
                .role_key
                .is_none_or(|role_key| self.role_key == role_key)
            && query
                .agent_runtime
                .is_none_or(|agent| self.agent_runtime == agent.slug())
    }
}

impl InstanceIndex {
    pub fn read_or_rebuild(data_dir: &Path) -> anyhow::Result<Self> {
        if let Ok(index) = Self::read(data_dir) {
            Ok(index)
        } else {
            let index = Self::rebuild(data_dir)?;
            index.write(data_dir)?;
            Ok(index)
        }
    }

    pub fn update_manifest(data_dir: &Path, manifest: &InstanceManifest) -> anyhow::Result<()> {
        let mut index = Self::read_or_rebuild(data_dir)?;
        index
            .instances
            .retain(|entry| entry.container_base != manifest.container_base);
        index
            .instances
            .push(InstanceIndexEntry::from_manifest(manifest));
        index.sort();
        index.write(data_dir)
    }

    pub fn remove(data_dir: &Path, container_base: &str) -> anyhow::Result<()> {
        let mut index = Self::read_or_rebuild(data_dir)?;
        index
            .instances
            .retain(|entry| entry.container_base != container_base);
        index.write(data_dir)
    }

    pub fn matching_manifests(
        data_dir: &Path,
        query: InstanceQuery<'_>,
    ) -> anyhow::Result<Vec<InstanceManifest>> {
        let index = Self::read_or_rebuild(data_dir)?;
        let mut manifests = Vec::new();
        for entry in index
            .instances
            .into_iter()
            .filter(|entry| entry.matches(query))
        {
            let state_dir = data_dir.join(&entry.container_base);
            let Ok(manifest) = InstanceManifest::read(&state_dir) else {
                continue;
            };
            if InstanceIndexEntry::from_manifest(&manifest).matches(query) {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(manifests)
    }

    fn read(data_dir: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(data_dir.join(INSTANCE_INDEX_FILE))?;
        let index: Self = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            index.version == INSTANCE_INDEX_VERSION,
            "unsupported instance index version {}",
            index.version
        );
        Ok(index)
    }

    fn rebuild(data_dir: &Path) -> anyhow::Result<Self> {
        let mut index = Self {
            version: INSTANCE_INDEX_VERSION,
            instances: Vec::new(),
        };
        if !data_dir.exists() {
            return Ok(index);
        }

        for entry in std::fs::read_dir(data_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let manifest = if let Ok(manifest) = InstanceManifest::read(&entry.path()) {
                manifest
            } else {
                let Some(manifest) = legacy_manifest_from_isolation(&entry.path())? else {
                    continue;
                };
                manifest.write(&entry.path())?;
                manifest
            };
            index
                .instances
                .push(InstanceIndexEntry::from_manifest(&manifest));
        }
        index.sort();
        Ok(index)
    }

    fn write(&self, data_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(data_dir.join(INSTANCE_INDEX_FILE), bytes)?;
        Ok(())
    }

    fn sort(&mut self) {
        self.instances
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn legacy_manifest_from_isolation(state_dir: &Path) -> anyhow::Result<Option<InstanceManifest>> {
    let records = crate::isolation::state::read_records(state_dir)?;
    let Some(first) = records.first() else {
        return Ok(None);
    };
    let Some(container_base) = state_dir.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    if !is_legacy_container_name(container_base) {
        return Ok(None);
    }

    let status = if records.iter().any(|record| {
        record.cleanup_status == crate::isolation::state::CleanupStatus::PreservedDirty
    }) {
        InstanceStatus::PreservedDirty
    } else if records.iter().any(|record| {
        record.cleanup_status == crate::isolation::state::CleanupStatus::PreservedUnpushed
    }) {
        InstanceStatus::PreservedUnpushed
    } else {
        InstanceStatus::Active
    };
    let now = now_rfc3339();
    Ok(Some(InstanceManifest {
        version: INSTANCE_MANIFEST_VERSION,
        legacy_name: true,
        instance_id: container_base.to_string(),
        container_base: container_base.to_string(),
        created_at: now.clone(),
        updated_at: now,
        workspace_name: (!first.workspace.starts_with('/')).then(|| first.workspace.clone()),
        workspace_label: first.workspace.clone(),
        workdir: first.mount_dst.clone(),
        host_workdir_fingerprint: host_path_fingerprint(&first.original_src),
        role_key: first.selector_key.clone(),
        role_display_name: first.selector_key.clone(),
        agent_runtime: Agent::Claude.slug().to_string(),
        role_source_git: String::new(),
        role_source_ref: None,
        image_tag: legacy_image_tag(&first.selector_key),
        status,
        last_attach_outcome: None,
        docker: DockerResources {
            role_container: container_base.to_string(),
            dind_container: format!("{container_base}-dind"),
            network: format!("{container_base}-net"),
            certs_volume: format!("{container_base}-dind-certs"),
        },
    }))
}

fn is_legacy_container_name(container_base: &str) -> bool {
    container_base.contains("__") || container_base.contains("-clone-")
}

fn legacy_image_tag(role_key: &str) -> String {
    role_key.split_once('/').map_or_else(
        || format!("jackin-{role_key}"),
        |(namespace, name)| format!("jackin-{namespace}__{name}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isolation::MountIsolation;
    use crate::isolation::state::{CleanupStatus, IsolationRecord};
    use tempfile::tempdir;

    #[test]
    fn writes_manifest_under_jackin_state_dir() {
        let temp = tempdir().unwrap();
        let mut manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        });
        manifest.mark_status(InstanceStatus::Running);

        manifest.write(temp.path()).unwrap();

        let body = std::fs::read_to_string(temp.path().join(".jackin/instance.json")).unwrap();
        assert!(body.contains(r#""version": 1"#));
        assert!(body.contains(r#""status": "running""#));
        assert!(body.contains(r#""role_key": "org/agent""#));
    }

    #[test]
    fn index_rebuilds_from_manifests_and_filters_by_query() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        let manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        });
        manifest
            .write(&data_dir.join("jackin-workspace-agent-k7p9m2xq"))
            .unwrap();

        let matches = InstanceIndex::matching_manifests(
            data_dir,
            InstanceQuery {
                workspace_name: Some("workspace"),
                workspace_label: "workspace",
                workdir: "/workspace",
                role_key: Some("org/agent"),
                agent_runtime: Some(Agent::Claude),
            },
        )
        .unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].container_base, "jackin-workspace-agent-k7p9m2xq");
        assert!(data_dir.join(INSTANCE_INDEX_FILE).exists());
    }

    #[test]
    fn index_update_replaces_existing_entry() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        let mut manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        });

        InstanceIndex::update_manifest(data_dir, &manifest).unwrap();
        manifest.mark_status(InstanceStatus::Running);
        InstanceIndex::update_manifest(data_dir, &manifest).unwrap();

        let index = InstanceIndex::read(data_dir).unwrap();
        assert_eq!(index.instances.len(), 1);
        assert_eq!(index.instances[0].status, InstanceStatus::Running);
    }

    #[test]
    fn index_rebuild_backfills_legacy_isolation_manifest() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        let state_dir = data_dir.join("jackin-chainargos__agent-brown-clone-1");
        crate::isolation::state::write_records(
            &state_dir,
            &[IsolationRecord {
                workspace: "chainargos-project".to_string(),
                mount_dst: "/workspace".to_string(),
                original_src: "/host/project".to_string(),
                isolation: MountIsolation::Worktree,
                worktree_path: "/host/worktree".to_string(),
                scratch_branch: "jackin/test".to_string(),
                base_commit: "abc123".to_string(),
                selector_key: "chainargos/agent-brown".to_string(),
                container_name: "jackin-chainargos__agent-brown-clone-1".to_string(),
                cleanup_status: CleanupStatus::PreservedDirty,
            }],
        )
        .unwrap();

        let index = InstanceIndex::read_or_rebuild(data_dir).unwrap();

        assert_eq!(index.instances.len(), 1);
        let manifest = InstanceManifest::read(&state_dir).unwrap();
        assert!(manifest.legacy_name);
        assert_eq!(
            manifest.container_base,
            "jackin-chainargos__agent-brown-clone-1"
        );
        assert_eq!(manifest.role_key, "chainargos/agent-brown");
        assert_eq!(manifest.status, InstanceStatus::PreservedDirty);
        assert_eq!(manifest.image_tag, "jackin-chainargos__agent-brown");
        assert!(manifest.host_workdir_fingerprint.starts_with("sha256:"));
    }
}
