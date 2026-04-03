use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use crate::workspace::{WorkspaceConfig, WorkspaceEdit, expand_tilde, validate_workspace_config};
use serde::{Deserialize, Serialize};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

pub use crate::workspace::MountConfig;

const BUILTIN_AGENTS: &[(&str, &str)] = &[
    (
        "agent-smith",
        "git@github.com:donbeave/jackin-agent-smith.git",
    ),
    (
        "the-architect",
        "git@github.com:donbeave/jackin-the-architect.git",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSource {
    pub git: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MountEntry {
    Mount(MountConfig),
    Scoped(BTreeMap<String, MountConfig>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerMounts(BTreeMap<String, MountEntry>);

impl DockerMounts {
    pub fn get(&self, key: &str) -> Option<&MountEntry> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut MountEntry> {
        self.0.get_mut(key)
    }

    pub fn insert(&mut self, key: String, value: MountEntry) -> Option<MountEntry> {
        self.0.insert(key, value)
    }

    pub fn remove(&mut self, key: &str) -> Option<MountEntry> {
        self.0.remove(key)
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
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}

impl AppConfig {
    pub fn load_or_init(paths: &JackinPaths) -> anyhow::Result<Self> {
        paths.ensure_base_dirs()?;

        let mut config = match std::fs::read_to_string(&paths.config_file) {
            Ok(contents) => toml::from_str(&contents)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };

        if config.sync_builtin_agents() {
            config.save(paths)?;
        }

        config.validate_workspaces()?;
        Ok(config)
    }

    /// Resolve an existing agent source or derive a new one from the selector.
    ///
    /// Returns `(source, is_new)`. When `is_new` is `true` the source has been
    /// inserted into the in-memory config but **not** persisted — the caller
    /// should call [`save`] after validating that the repository is reachable.
    pub fn resolve_agent_source(
        &mut self,
        selector: &ClassSelector,
    ) -> anyhow::Result<(AgentSource, bool)> {
        if let Some(source) = self.agents.get(&selector.key()) {
            return Ok((source.clone(), false));
        }

        let namespace = selector
            .namespace
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("unknown selector {}", selector.key()))?;

        let source = AgentSource {
            git: format!("git@github.com:{namespace}/jackin-{}.git", selector.name),
        };
        self.agents.insert(selector.key(), source.clone());
        Ok((source, true))
    }

    pub fn save(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        std::fs::write(&paths.config_file, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn resolve_mounts(&self, selector: &ClassSelector) -> Vec<(String, MountConfig)> {
        let mut by_name: BTreeMap<String, MountConfig> = BTreeMap::new();

        // Priority order: global < wildcard < exact (later inserts override earlier)
        let scopes = [
            None,                                                    // global
            selector.namespace.as_ref().map(|ns| format!("{ns}/*")), // wildcard
            Some(selector.key()),                                    // exact
        ];

        for scope in &scopes {
            let entries = match scope {
                None => {
                    let mut map = BTreeMap::new();
                    for (name, entry) in self.docker.mounts.iter() {
                        if let MountEntry::Mount(m) = entry {
                            map.insert(name.clone(), m.clone());
                        }
                    }
                    map
                }
                Some(scope_key) => match self.docker.mounts.get(scope_key) {
                    Some(MountEntry::Scoped(scope_map)) => scope_map.clone(),
                    _ => continue,
                },
            };

            for (name, mount) in entries {
                by_name.insert(name, mount);
            }
        }

        by_name.into_iter().collect()
    }

    pub fn expand_and_validate_named_mounts(
        mounts: &[(String, MountConfig)],
    ) -> anyhow::Result<Vec<MountConfig>> {
        let expanded: Vec<MountConfig> = mounts
            .iter()
            .map(|(_, mount)| MountConfig {
                src: expand_tilde(&mount.src),
                dst: mount.dst.clone(),
                readonly: mount.readonly,
            })
            .collect();
        crate::workspace::validate_mounts(&expanded)?;
        Ok(expanded)
    }

    pub fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            self.docker
                .mounts
                .insert(name.to_string(), MountEntry::Mount(mount));
        } else {
            match self.docker.mounts.entry(scope_key.to_string()) {
                Entry::Occupied(mut entry) => {
                    if let MountEntry::Scoped(map) = entry.get_mut() {
                        map.insert(name.to_string(), mount);
                    }
                }
                Entry::Vacant(entry) => {
                    let mut map = BTreeMap::new();
                    map.insert(name.to_string(), mount);
                    entry.insert(MountEntry::Scoped(map));
                }
            }
        }
    }

    pub fn remove_mount(&mut self, name: &str, scope: Option<&str>) -> bool {
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            self.docker.mounts.remove(name).is_some()
        } else {
            match self.docker.mounts.get_mut(scope_key) {
                Some(MountEntry::Scoped(map)) => {
                    let removed = map.remove(name).is_some();
                    if map.is_empty() {
                        self.docker.mounts.remove(scope_key);
                    }
                    removed
                }
                _ => false,
            }
        }
    }

    pub fn list_mounts(&self) -> Vec<(String, String, &MountConfig)> {
        let mut result = Vec::new();
        for (key, entry) in self.docker.mounts.iter() {
            match entry {
                MountEntry::Mount(m) => {
                    result.push(("(global)".to_string(), key.clone(), m));
                }
                MountEntry::Scoped(map) => {
                    for (name, m) in map {
                        result.push((key.clone(), name.clone(), m));
                    }
                }
            }
        }
        result
    }

    pub fn add_workspace(&mut self, name: &str, workspace: WorkspaceConfig) -> anyhow::Result<()> {
        if self.workspaces.contains_key(name) {
            anyhow::bail!("workspace {name:?} already exists; use `workspace edit`");
        }
        validate_workspace_config(name, &workspace)?;
        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }

    pub fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
        let mut seen_upsert_destinations = std::collections::HashSet::new();
        for mount in &edit.upsert_mounts {
            if !seen_upsert_destinations.insert(mount.dst.as_str()) {
                anyhow::bail!("duplicate workspace edit mount destination: {}", mount.dst);
            }
        }

        let current = self
            .workspaces
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?;
        let mut workspace = current.clone();

        if let Some(workdir) = edit.workdir {
            workspace.workdir = workdir;
        }

        for dst in edit.remove_destinations {
            let original_len = workspace.mounts.len();
            workspace.mounts.retain(|mount| mount.dst != dst);
            if workspace.mounts.len() == original_len {
                anyhow::bail!("unknown workspace mount destination: {dst}");
            }
        }

        for mount in edit.upsert_mounts {
            if let Some(existing) = workspace
                .mounts
                .iter_mut()
                .find(|existing| existing.dst == mount.dst)
            {
                *existing = mount;
            } else {
                workspace.mounts.push(mount);
            }
        }

        for selector in edit.allowed_agents_to_add {
            if !workspace
                .allowed_agents
                .iter()
                .any(|existing| existing == &selector)
            {
                workspace.allowed_agents.push(selector);
            }
        }

        for selector in edit.allowed_agents_to_remove {
            workspace
                .allowed_agents
                .retain(|existing| existing != &selector);
        }

        if let Some(default_agent) = edit.default_agent {
            workspace.default_agent = default_agent;
        }

        validate_workspace_config(name, &workspace)?;
        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        self.workspaces
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))
    }

    pub fn global_mounts(&self) -> Vec<MountConfig> {
        self.docker
            .mounts
            .iter()
            .filter_map(|(_, entry)| match entry {
                MountEntry::Mount(m) => Some(m.clone()),
                MountEntry::Scoped(_) => None,
            })
            .collect()
    }

    pub fn list_workspaces(&self) -> Vec<(&str, &WorkspaceConfig)> {
        self.workspaces
            .iter()
            .map(|(name, workspace)| (name.as_str(), workspace))
            .collect()
    }

    fn validate_workspaces(&self) -> anyhow::Result<()> {
        for (name, workspace) in &self.workspaces {
            validate_workspace_config(name, workspace)?;
        }
        Ok(())
    }

    /// Ensures all built-in agent entries match the current binary version.
    /// Returns `true` if any entries were added or updated.
    fn sync_builtin_agents(&mut self) -> bool {
        let mut changed = false;
        for &(name, git) in BUILTIN_AGENTS {
            let expected = AgentSource {
                git: git.to_string(),
            };
            match self.agents.get(name) {
                Some(existing) if existing.git == expected.git => {}
                _ => {
                    self.agents.insert(name.to_string(), expected);
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_writes_builtin_agent_entries() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let config = AppConfig::load_or_init(&paths).unwrap();

        assert_eq!(
            config.agents.get("agent-smith").unwrap().git,
            "git@github.com:donbeave/jackin-agent-smith.git"
        );
        assert_eq!(
            config.agents.get("the-architect").unwrap().git,
            "git@github.com:donbeave/jackin-the-architect.git"
        );
        assert!(paths.config_file.exists());
    }

    #[test]
    fn sync_updates_stale_builtin_entries_and_preserves_user_agents() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[agents.agent-smith]
git = "git@github.com:old/wrong-url.git"

[agents."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
"#,
        )
        .unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();

        // Built-in entries are corrected
        assert_eq!(
            config.agents.get("agent-smith").unwrap().git,
            "git@github.com:donbeave/jackin-agent-smith.git"
        );
        // Missing built-in entries are added
        assert_eq!(
            config.agents.get("the-architect").unwrap().git,
            "git@github.com:donbeave/jackin-the-architect.git"
        );
        // User-added entries are preserved
        assert_eq!(
            config.agents.get("chainargos/agent-brown").unwrap().git,
            "git@github.com:chainargos/jackin-agent-brown.git"
        );

        // Config file is updated on disk
        let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(persisted.contains("donbeave/jackin-agent-smith.git"));
        assert!(persisted.contains("donbeave/jackin-the-architect.git"));
        assert!(persisted.contains("chainargos/jackin-agent-brown.git"));
    }

    #[test]
    fn sync_does_not_rewrite_config_when_already_current() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        // First load creates the file
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_before = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        // Small delay so mtime would differ if rewritten
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second load should not rewrite
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_after = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(mtime_before, mtime_after);
    }

    #[test]
    fn resolve_agent_source_adds_owner_repo_on_first_use() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "the-architect");

        let (source, is_new) = config.resolve_agent_source(&selector).unwrap();

        assert_eq!(
            source.git,
            "git@github.com:chainargos/jackin-the-architect.git"
        );
        assert!(is_new);

        // Not yet persisted — caller must save explicitly
        config.save(&paths).unwrap();
        assert!(
            std::fs::read_to_string(&paths.config_file)
                .unwrap()
                .contains("[agents.\"chainargos/the-architect\"]")
        );
    }

    // --- Task 3: Deserialization tests ---

    #[test]
    fn deserializes_global_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/claude/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/claude/.gradle/wrapper", readonly = true }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let mounts = &config.docker.mounts;
        match mounts.get("gradle-cache").unwrap() {
            MountEntry::Mount(m) => {
                assert_eq!(m.src, "~/.gradle/caches");
                assert_eq!(m.dst, "/home/claude/.gradle/caches");
                assert!(!m.readonly);
            }
            MountEntry::Scoped(_) => panic!("expected MountEntry::Mount"),
        }
        match mounts.get("gradle-wrapper").unwrap() {
            MountEntry::Mount(m) => assert!(m.readonly),
            MountEntry::Scoped(_) => panic!("expected MountEntry::Mount"),
        }
    }

    #[test]
    fn deserializes_scoped_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let mounts = &config.docker.mounts;
        match mounts.get("chainargos/*").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("chainargos-secrets").unwrap();
                assert_eq!(m.dst, "/secrets");
                assert!(m.readonly);
            }
            MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
        }
        match mounts.get("chainargos/agent-brown").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("brown-config").unwrap();
                assert_eq!(m.dst, "/config");
                assert!(!m.readonly);
            }
            MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
        }
    }

    #[test]
    fn deserializes_saved_workspaces() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/Users/donbeave/Projects/chainargos/big-monorepo"
default_agent = "agent-smith"
allowed_agents = ["agent-smith", "chainargos/the-architect"]

[[workspaces.big-monorepo.mounts]]
src = "/Users/donbeave/Projects/chainargos/big-monorepo"
dst = "/Users/donbeave/Projects/chainargos/big-monorepo"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/cache"
dst = "/workspace/cache"
readonly = true
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let workspace = config.workspaces.get("big-monorepo").unwrap();

        assert_eq!(
            workspace.workdir,
            "/Users/donbeave/Projects/chainargos/big-monorepo"
        );
        assert_eq!(workspace.mounts.len(), 2);
        assert_eq!(workspace.default_agent.as_deref(), Some("agent-smith"));
        assert_eq!(workspace.allowed_agents.len(), 2);
        assert!(workspace.mounts[1].readonly);
    }

    #[test]
    fn rejects_workspace_with_workdir_outside_mounts() {
        let temp = tempdir().unwrap();

        let workspace = crate::workspace::WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: temp.path().display().to_string(),
                dst: "/workspace/src".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
                last_agent: None,
        };

        let error =
            crate::workspace::validate_workspace_config("big-monorepo", &workspace).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("must be equal to or inside one of the workspace mount destinations")
        );
    }

    #[test]
    fn edit_workspace_does_not_persist_invalid_mutation() {
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        let src = temp.path().display().to_string();

        config
            .add_workspace(
                "big-monorepo",
                WorkspaceConfig {
                    workdir: "/workspace/project".to_string(),
                    mounts: vec![MountConfig {
                        src,
                        dst: "/workspace/project".to_string(),
                        readonly: false,
                    }],
                    allowed_agents: vec![],
                    default_agent: None,
                last_agent: None,
                },
            )
            .unwrap();

        let error = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    workdir: Some("/workspace/missing".to_string()),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("must be equal to or inside one of the workspace mount destinations")
        );
        assert_eq!(
            config.workspaces.get("big-monorepo").unwrap().workdir,
            "/workspace/project"
        );
    }

    #[test]
    fn load_or_init_rejects_invalid_saved_workspace() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            &paths.config_file,
            r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp"
dst = "/workspace/src"
"#,
        )
        .unwrap();

        let error = AppConfig::load_or_init(&paths).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("must be equal to or inside one of the workspace mount destinations")
        );
    }

    #[test]
    fn load_or_init_rejects_invalid_persisted_workspace() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mount_src = temp.path().join("workspace-src");
        std::fs::create_dir_all(&mount_src).unwrap();

        let toml_str = format!(
            r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[workspaces.broken]
workdir = "/workspace/project"

[[workspaces.broken.mounts]]
src = "{}"
dst = "/workspace/src"
"#,
            mount_src.display()
        );

        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, toml_str).unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        assert!(err.to_string().contains("workspace \"broken\" workdir must be equal to or inside one of the workspace mount destinations"));
    }

    #[test]
    fn edit_workspace_leaves_original_value_when_validation_fails() {
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: temp.path().display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec!["agent-smith".to_string()],
            default_agent: Some("agent-smith".to_string()),
                last_agent: None,
        };
        config
            .add_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    workdir: Some("/workspace/elsewhere".to_string()),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("must be equal to or inside one of the workspace mount destinations")
        );
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn add_workspace_rejects_duplicate_name_and_preserves_existing_value() {
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: temp.path().display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
                last_agent: None,
        };
        config
            .add_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .add_workspace(
                "big-monorepo",
                WorkspaceConfig {
                    workdir: "/workspace/other".to_string(),
                    mounts: vec![MountConfig {
                        src: temp.path().display().to_string(),
                        dst: "/workspace/other".to_string(),
                        readonly: true,
                    }],
                    allowed_agents: vec!["agent-smith".to_string()],
                    default_agent: Some("agent-smith".to_string()),
                last_agent: None,
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("already exists"));
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn edit_workspace_rejects_duplicate_upsert_destinations() {
        let temp = tempdir().unwrap();
        let original_src = temp.path().join("project");
        let first_upsert = temp.path().join("cache-a");
        let second_upsert = temp.path().join("cache-b");
        std::fs::create_dir_all(&original_src).unwrap();
        std::fs::create_dir_all(&first_upsert).unwrap();
        std::fs::create_dir_all(&second_upsert).unwrap();

        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: original_src.display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
                last_agent: None,
        };
        config
            .add_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    upsert_mounts: vec![
                        MountConfig {
                            src: first_upsert.display().to_string(),
                            dst: "/workspace/cache".to_string(),
                            readonly: false,
                        },
                        MountConfig {
                            src: second_upsert.display().to_string(),
                            dst: "/workspace/cache".to_string(),
                            readonly: true,
                        },
                    ],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("duplicate workspace edit mount destination")
        );
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn edit_workspace_rejects_missing_remove_destination() {
        let temp = tempdir().unwrap();
        let original_src = temp.path().join("project");
        std::fs::create_dir_all(&original_src).unwrap();

        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: original_src.display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
                last_agent: None,
        };
        config
            .add_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    remove_destinations: vec!["/workspace/missing".to_string()],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("unknown workspace mount destination")
        );
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn remove_workspace_errors_when_missing() {
        let mut config = AppConfig::default();

        let err = config.remove_workspace("missing").unwrap_err();

        assert!(err.to_string().contains("unknown workspace missing"));
    }

    // --- Task 4: Resolution tests ---

    #[test]
    fn resolve_mounts_collects_global_and_matching_scopes() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "/tmp/gradle-caches", dst = "/home/claude/.gradle/caches" }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "/tmp/chainargos-secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "/tmp/chainargos-brown", dst = "/config" }

[docker.mounts."other/*"]
other-data = { src = "/tmp/other", dst = "/other" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 3);
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/home/claude/.gradle/caches")
        );
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/secrets" && m.readonly)
        );
        assert!(
            resolved
                .iter()
                .any(|(_, m)| m.dst == "/config" && !m.readonly)
        );
    }

    #[test]
    fn resolve_mounts_exact_overrides_global_with_same_name() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
shared = { src = "/tmp/global", dst = "/data" }

[docker.mounts."chainargos/agent-brown"]
shared = { src = "/tmp/specific", dst = "/data" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "agent-brown");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].1.src, "/tmp/specific");
    }

    #[test]
    fn resolve_mounts_returns_empty_when_no_mounts_configured() {
        let config = AppConfig::default();
        let selector = ClassSelector::new(None, "agent-smith");
        let resolved = config.resolve_mounts(&selector);
        assert!(resolved.is_empty());
    }

    // --- Task 5: Validation tests ---

    #[test]
    fn validate_mounts_rejects_missing_src() {
        let mounts = vec![(
            "test-mount".to_string(),
            MountConfig {
                src: "/nonexistent/path/that/does/not/exist".to_string(),
                dst: "/data".to_string(),
                readonly: false,
            },
        )];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(
            err.to_string()
                .contains("/nonexistent/path/that/does/not/exist")
        );
    }

    #[test]
    fn validate_mounts_rejects_relative_src() {
        let mounts = vec![(
            "test-mount".to_string(),
            MountConfig {
                src: ".".to_string(),
                dst: "/data".to_string(),
                readonly: false,
            },
        )];

        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();

        assert!(err.to_string().contains("mount source must be absolute"));
    }

    #[test]
    fn validate_mounts_rejects_relative_dst() {
        let temp = tempdir().unwrap();
        let mounts = vec![(
            "test-mount".to_string(),
            MountConfig {
                src: temp.path().display().to_string(),
                dst: "relative/path".to_string(),
                readonly: false,
            },
        )];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn validate_mounts_rejects_duplicate_dst() {
        let temp = tempdir().unwrap();
        let src = temp.path().display().to_string();
        let mounts = vec![
            (
                "mount-a".to_string(),
                MountConfig {
                    src: src.clone(),
                    dst: "/data".to_string(),
                    readonly: false,
                },
            ),
            (
                "mount-b".to_string(),
                MountConfig {
                    src,
                    dst: "/data".to_string(),
                    readonly: true,
                },
            ),
        ];
        let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_mounts_expands_tilde_in_src() {
        let home = std::env::var("HOME").unwrap();
        let mounts = vec![(
            "home-mount".to_string(),
            MountConfig {
                src: "~".to_string(),
                dst: "/home-mount".to_string(),
                readonly: true,
            },
        )];
        let validated = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap();
        assert_eq!(validated[0].src, home);
    }

    #[test]
    fn resolve_mounts_matches_exact_scope_for_unscoped_selector() {
        let toml_str = r#"
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[docker.mounts]
global-data = { src = "/tmp/global", dst = "/global" }

[docker.mounts."agent-smith"]
smith-data = { src = "/tmp/smith", dst = "/smith" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let selector = ClassSelector::new(None, "agent-smith");
        let resolved = config.resolve_mounts(&selector);
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|(_, m)| m.dst == "/global"));
        assert!(resolved.iter().any(|(_, m)| m.dst == "/smith"));
    }
}
