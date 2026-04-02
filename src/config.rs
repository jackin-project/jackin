use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use crate::workspace::{expand_tilde, validate_workspace_config, WorkspaceConfig, WorkspaceEdit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::workspace::MountConfig;

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
pub struct DockerMounts(pub BTreeMap<String, MountEntry>);

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

        if !paths.config_file.exists() {
            let config = Self::default_config();
            config.save(paths)?;
            return Ok(config);
        }

        let contents = std::fs::read_to_string(&paths.config_file)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn resolve_or_register(
        &mut self,
        selector: &ClassSelector,
        paths: &JackinPaths,
    ) -> anyhow::Result<AgentSource> {
        if let Some(source) = self.agents.get(&selector.key()) {
            return Ok(source.clone());
        }

        let namespace = selector
            .namespace
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("unknown selector {}", selector.key()))?;

        let source = AgentSource {
            git: format!("git@github.com:{namespace}/jackin-{}.git", selector.name),
        };
        self.agents.insert(selector.key(), source.clone());
        self.save(paths)?;
        Ok(source)
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
                    for (name, entry) in &self.docker.mounts.0 {
                        if let MountEntry::Mount(m) = entry {
                            map.insert(name.clone(), m.clone());
                        }
                    }
                    map
                }
                Some(scope_key) => match self.docker.mounts.0.get(scope_key) {
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

    pub fn validate_mounts(mounts: &[(String, MountConfig)]) -> anyhow::Result<Vec<MountConfig>> {
        let mut validated = Vec::new();
        let mut seen_dst = std::collections::HashSet::new();

        for (name, mount) in mounts {
            let expanded_src = expand_tilde(&mount.src);
            if !std::path::Path::new(&expanded_src).exists() {
                anyhow::bail!(
                    "mount {:?} source does not exist: {} (expanded from {:?})",
                    name,
                    expanded_src,
                    mount.src
                );
            }
            if !mount.dst.starts_with('/') {
                anyhow::bail!(
                    "mount {:?} destination must be an absolute path: {}",
                    name,
                    mount.dst
                );
            }
            if !seen_dst.insert(mount.dst.clone()) {
                anyhow::bail!(
                    "duplicate mount destination {:?} in mount {:?}",
                    mount.dst,
                    name
                );
            }
            validated.push(MountConfig {
                src: expanded_src,
                dst: mount.dst.clone(),
                readonly: mount.readonly,
            });
        }
        Ok(validated)
    }

    pub fn add_mount(&mut self, name: &str, mount: MountConfig, scope: Option<&str>) {
        let scope_key = scope.unwrap_or("");
        if scope_key.is_empty() {
            self.docker
                .mounts
                .0
                .insert(name.to_string(), MountEntry::Mount(mount));
        } else {
            match self.docker.mounts.0.entry(scope_key.to_string()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if let MountEntry::Scoped(map) = entry.get_mut() {
                        map.insert(name.to_string(), mount);
                    }
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
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
            self.docker.mounts.0.remove(name).is_some()
        } else {
            match self.docker.mounts.0.get_mut(scope_key) {
                Some(MountEntry::Scoped(map)) => {
                    let removed = map.remove(name).is_some();
                    if map.is_empty() {
                        self.docker.mounts.0.remove(scope_key);
                    }
                    removed
                }
                _ => false,
            }
        }
    }

    pub fn list_mounts(&self) -> Vec<(String, String, &MountConfig)> {
        let mut result = Vec::new();
        for (key, entry) in &self.docker.mounts.0 {
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
        validate_workspace_config(name, &workspace)?;
        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }

    pub fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
        let workspace = self
            .workspaces
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?;

        if let Some(workdir) = edit.workdir {
            workspace.workdir = workdir;
        }

        for dst in edit.remove_destinations {
            workspace.mounts.retain(|mount| mount.dst != dst);
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

        validate_workspace_config(name, workspace)?;
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &str) -> bool {
        self.workspaces.remove(name).is_some()
    }

    pub fn list_workspaces(&self) -> Vec<(&str, &WorkspaceConfig)> {
        self.workspaces
            .iter()
            .map(|(name, workspace)| (name.as_str(), workspace))
            .collect()
    }

    pub fn global_mounts(&self) -> Vec<MountConfig> {
        self.docker
            .mounts
            .0
            .iter()
            .filter_map(|(_, entry)| match entry {
                MountEntry::Mount(mount) => Some(mount.clone()),
                MountEntry::Scoped(_) => None,
            })
            .collect()
    }

    fn default_config() -> Self {
        let mut agents = BTreeMap::new();
        agents.insert(
            "agent-smith".to_string(),
            AgentSource {
                git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
            },
        );
        Self {
            agents,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_writes_default_agent_smith_entry() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let config = AppConfig::load_or_init(&paths).unwrap();

        assert_eq!(
            config.agents.get("agent-smith").unwrap().git,
            "git@github.com:donbeave/jackin-agent-smith.git"
        );
        assert!(paths.config_file.exists());
    }

    #[test]
    fn resolve_or_register_adds_owner_repo_on_first_use() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "the-architect");

        let source = config.resolve_or_register(&selector, &paths).unwrap();

        assert_eq!(
            source.git,
            "git@github.com:chainargos/jackin-the-architect.git"
        );
        assert!(std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("[agents.\"chainargos/the-architect\"]"));
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
        let mounts = &config.docker.mounts.0;
        match mounts.get("gradle-cache").unwrap() {
            MountEntry::Mount(m) => {
                assert_eq!(m.src, "~/.gradle/caches");
                assert_eq!(m.dst, "/home/claude/.gradle/caches");
                assert!(!m.readonly);
            }
            _ => panic!("expected MountEntry::Mount"),
        }
        match mounts.get("gradle-wrapper").unwrap() {
            MountEntry::Mount(m) => assert!(m.readonly),
            _ => panic!("expected MountEntry::Mount"),
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
        let mounts = &config.docker.mounts.0;
        match mounts.get("chainargos/*").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("chainargos-secrets").unwrap();
                assert_eq!(m.dst, "/secrets");
                assert!(m.readonly);
            }
            _ => panic!("expected MountEntry::Scoped"),
        }
        match mounts.get("chainargos/agent-brown").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("brown-config").unwrap();
                assert_eq!(m.dst, "/config");
                assert!(!m.readonly);
            }
            _ => panic!("expected MountEntry::Scoped"),
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
        };

        let error =
            crate::workspace::validate_workspace_config("big-monorepo", &workspace).unwrap_err();

        assert!(error
            .to_string()
            .contains("must be equal to or inside one of the workspace mount destinations"));
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
        assert!(resolved
            .iter()
            .any(|(_, m)| m.dst == "/home/claude/.gradle/caches"));
        assert!(resolved
            .iter()
            .any(|(_, m)| m.dst == "/secrets" && m.readonly));
        assert!(resolved
            .iter()
            .any(|(_, m)| m.dst == "/config" && !m.readonly));
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
        let err = AppConfig::validate_mounts(&mounts).unwrap_err();
        assert!(err
            .to_string()
            .contains("/nonexistent/path/that/does/not/exist"));
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
        let err = AppConfig::validate_mounts(&mounts).unwrap_err();
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
        let err = AppConfig::validate_mounts(&mounts).unwrap_err();
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
        let validated = AppConfig::validate_mounts(&mounts).unwrap();
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
