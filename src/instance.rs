use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentState {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
    pub jackin_dir: PathBuf,
    pub plugins_json: PathBuf,
    pub gh_config_dir: PathBuf,
}

#[derive(Debug, Serialize)]
struct PluginState<'a> {
    plugins: &'a [String],
}

impl AgentState {
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &AgentManifest,
    ) -> anyhow::Result<Self> {
        let root = paths.data_dir.join(container_name);
        let claude_dir = root.join(".claude");
        let claude_json = root.join(".claude.json");
        let jackin_dir = root.join(".jackin");
        let plugins_json = jackin_dir.join("plugins.json");
        let gh_config_dir = root.join(".config/gh");

        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&jackin_dir)?;
        std::fs::create_dir_all(&gh_config_dir)?;
        if !claude_json.exists() {
            std::fs::write(&claude_json, "{}")?;
        }

        std::fs::write(
            &plugins_json,
            serde_json::to_string_pretty(&PluginState {
                plugins: &manifest.claude.plugins,
            })?,
        )?;

        Ok(Self {
            root,
            claude_dir,
            claude_json,
            jackin_dir,
            plugins_json,
            gh_config_dir,
        })
    }
}

pub fn runtime_slug(selector: &ClassSelector) -> String {
    selector.namespace.as_ref().map_or_else(
        || selector.name.clone(),
        |namespace| format!("{namespace}__{}", selector.name),
    )
}

pub fn primary_container_name(selector: &ClassSelector) -> String {
    format!("jackin-{}", runtime_slug(selector))
}

pub fn next_container_name(selector: &ClassSelector, existing: &[String]) -> String {
    let primary = primary_container_name(selector);
    if !existing.iter().any(|name| name == &primary) {
        return primary;
    }

    let mut clone_index = 1;
    loop {
        let candidate = format!("{primary}-clone-{clone_index}");
        if !existing.iter().any(|name| name == &candidate) {
            return candidate;
        }
        clone_index += 1;
    }
}

pub fn class_family_matches(selector: &ClassSelector, container_name: &str) -> bool {
    let primary = primary_container_name(selector);
    container_name == primary || container_name.starts_with(&format!("{primary}-clone-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn picks_next_clone_name() {
        let selector = ClassSelector::new(None, "agent-smith");
        let existing = vec![
            "jackin-agent-smith".to_string(),
            "jackin-agent-smith-clone-1".to_string(),
        ];

        let name = next_container_name(&selector, &existing);

        assert_eq!(name, "jackin-agent-smith-clone-2");
    }

    #[test]
    fn distinguishes_namespaced_and_flat_class_container_names() {
        let namespaced = ClassSelector::new(Some("chainargos"), "the-architect");
        let flat = ClassSelector::new(None, "chainargos-the-architect");

        assert_ne!(primary_container_name(&namespaced), primary_container_name(&flat));
    }

    #[test]
    fn prepares_persisted_claude_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();
        let manifest = crate::manifest::AgentManifest::load(temp.path()).unwrap();

        let state = AgentState::prepare(&paths, "jackin-agent-smith", &manifest).unwrap();

        assert!(state.claude_dir.is_dir());
        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
    }

    #[test]
    fn prepares_plugins_json_for_runtime_bootstrap() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\", \"feature-dev@claude-plugins-official\"]\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM donbeave/jackin-construct:trixie\n",
        )
        .unwrap();

        let manifest = crate::manifest::AgentManifest::load(temp.path()).unwrap();
        let state = AgentState::prepare(&paths, "jackin-agent-smith", &manifest).unwrap();

        assert!(state.jackin_dir.is_dir());
        assert_eq!(
            std::fs::read_to_string(&state.plugins_json).unwrap(),
            "{\n  \"plugins\": [\n    \"code-review@claude-plugins-official\",\n    \"feature-dev@claude-plugins-official\"\n  ]\n}"
        );
    }
}
