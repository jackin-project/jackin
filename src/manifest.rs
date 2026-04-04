use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVarDecl {
    #[serde(rename = "default")]
    pub default_value: Option<String>,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub skippable: bool,
    pub prompt: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    pub pre_launch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub plugins: Vec<String>,
}

impl AgentManifest {
    pub fn load(repo_dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = repo_dir.join("jackin.agent.toml");
        let contents = std::fs::read_to_string(&manifest_path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_string(), |id| id.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_manifest_with_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.dockerfile, "Dockerfile");
        assert_eq!(manifest.claude.plugins.len(), 1);
        assert!(manifest.identity.is_none());
    }

    #[test]
    fn loads_manifest_with_identity() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Agent Smith\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.identity.as_ref().unwrap().name, "Agent Smith");
    }

    #[test]
    fn display_name_uses_identity_when_present() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Agent Smith\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "Agent Smith");
    }

    #[test]
    fn display_name_falls_back_to_class_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "agent-smith");
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\nunknown_field = true\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let error = AgentManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_claude_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\ntypo = \"oops\"\n",
        )
        .unwrap();

        let error = AgentManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_identity_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[identity]\nname = \"Smith\"\ntypo = true\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let error = AgentManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn loads_manifest_with_hooks() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(
            manifest.hooks.as_ref().unwrap().pre_launch.as_deref(),
            Some("hooks/pre-launch.sh")
        );
    }

    #[test]
    fn loads_manifest_without_hooks() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert!(manifest.hooks.is_none());
    }

    #[test]
    fn rejects_unknown_hooks_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\npost_launch = \"bad\"\n",
        )
        .unwrap();

        let error = AgentManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn loads_manifest_with_static_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[env.CLAUDE_ENV]\ndefault = \"docker\"\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.env.len(), 1);
        let var = &manifest.env["CLAUDE_ENV"];
        assert_eq!(var.default_value.as_deref(), Some("docker"));
        assert!(!var.interactive);
    }

    #[test]
    fn loads_manifest_with_interactive_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[env.PROJECT]\ninteractive = true\nprompt = \"Select a project:\"\noptions = [\"project1\", \"project2\"]\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        let var = &manifest.env["PROJECT"];
        assert!(var.interactive);
        assert_eq!(var.prompt.as_deref(), Some("Select a project:"));
        assert_eq!(var.options, vec!["project1", "project2"]);
    }

    #[test]
    fn loads_manifest_with_env_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[env.PROJECT]\ninteractive = true\nprompt = \"Select:\"\noptions = [\"a\", \"b\"]\n\n[env.BRANCH]\ninteractive = true\ndepends_on = [\"env.PROJECT\"]\nprompt = \"Branch:\"\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        let var = &manifest.env["BRANCH"];
        assert_eq!(var.depends_on, vec!["env.PROJECT"]);
    }

    #[test]
    fn loads_manifest_with_skippable_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[env.API_KEY]\ninteractive = true\nskippable = true\nprompt = \"API key (optional):\"\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        let var = &manifest.env["API_KEY"];
        assert!(var.skippable);
    }

    #[test]
    fn loads_manifest_without_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert!(manifest.env.is_empty());
    }

    #[test]
    fn rejects_unknown_env_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n\n[env.FOO]\ndefault = \"bar\"\ntypo = true\n",
        )
        .unwrap();

        let error = AgentManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }
}
