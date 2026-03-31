use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSource {
    pub git: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSource>,
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
            git: format!("git@github.com:{namespace}/{}.git", selector.name),
        };
        self.agents.insert(selector.key(), source.clone());
        self.save(paths)?;
        Ok(source)
    }

    pub fn save(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        std::fs::write(&paths.config_file, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    fn default_config() -> Self {
        let mut agents = BTreeMap::new();
        agents.insert(
            "smith".to_string(),
            AgentSource {
                git: "git@github.com:donbeave/smith.git".to_string(),
            },
        );
        Self { agents }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_writes_default_smith_entry() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let config = AppConfig::load_or_init(&paths).unwrap();

        assert_eq!(
            config.agents.get("smith").unwrap().git,
            "git@github.com:donbeave/smith.git"
        );
        assert!(paths.config_file.exists());
    }

    #[test]
    fn resolve_or_register_adds_owner_repo_on_first_use() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "smith");

        let source = config.resolve_or_register(&selector, &paths).unwrap();

        assert_eq!(source.git, "git@github.com:chainargos/smith.git");
        assert!(std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("[agents.\"chainargos/smith\"]"));
    }
}
