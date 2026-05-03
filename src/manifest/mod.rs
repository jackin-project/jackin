pub use crate::env_model::{
    JACKIN_DIND_HOSTNAME_ENV_NAME, JACKIN_RUNTIME_ENV_NAME, JACKIN_RUNTIME_ENV_VALUE,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod validate;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    #[serde(default)]
    pub harness: Option<HarnessConfig>,
    #[serde(default)]
    pub claude: Option<ClaudeConfig>,
    #[serde(default)]
    pub codex: Option<CodexConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessConfig {
    pub supported: Vec<crate::harness::Harness>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    /// Optional model override; passed into the generated config.toml
    /// when present, otherwise Codex's own default is used.
    #[serde(default)]
    pub model: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeMarketplaceConfig {
    pub source: String,
    #[serde(default)]
    pub sparse: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub marketplaces: Vec<ClaudeMarketplaceConfig>,
    #[serde(default)]
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ManifestWarning {
    pub message: String,
}

impl RoleManifest {
    pub fn load(repo_dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = repo_dir.join("jackin.role.toml");
        let contents = std::fs::read_to_string(&manifest_path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_string(), |id| id.name.clone())
    }

    /// Returns the harnesses this manifest supports. Legacy manifests
    /// without a `[harness]` table default to claude-only.
    pub fn supported_harnesses(&self) -> Vec<crate::harness::Harness> {
        self.harness.as_ref().map_or_else(
            || vec![crate::harness::Harness::Claude],
            |h| h.supported.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_manifest_with_harness_table() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(
            m.supported_harnesses(),
            vec![
                crate::harness::Harness::Claude,
                crate::harness::Harness::Codex
            ]
        );
        assert!(m.codex.is_some());
    }

    #[test]
    fn legacy_manifest_without_harness_table_defaults_to_claude_only() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(
            m.supported_harnesses(),
            vec![crate::harness::Harness::Claude]
        );
    }

    #[test]
    fn loads_codex_only_manifest() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

[codex]
model = "gpt-5"
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(
            m.supported_harnesses(),
            vec![crate::harness::Harness::Codex]
        );
        assert_eq!(m.codex.as_ref().unwrap().model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn rejects_unknown_harness_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "amp"]

[claude]
plugins = []
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        assert!(err.to_string().contains("amp") || err.to_string().contains("unknown"));
    }

    #[test]
    fn loads_manifest_with_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.dockerfile, "Dockerfile");
        assert!(manifest.claude.as_ref().unwrap().marketplaces.is_empty());
        assert_eq!(manifest.claude.as_ref().unwrap().plugins.len(), 1);
        assert!(manifest.identity.is_none());
    }

    #[test]
    fn loads_manifest_with_marketplaces_and_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(
            manifest.claude.as_ref().unwrap().plugins,
            vec!["superpowers@superpowers-marketplace"]
        );
        assert_eq!(manifest.claude.as_ref().unwrap().marketplaces.len(), 1);
        assert_eq!(
            manifest.claude.as_ref().unwrap().marketplaces[0],
            ClaudeMarketplaceConfig {
                source: "obra/superpowers-marketplace".to_string(),
                sparse: vec!["plugins".to_string(), ".claude-plugin".to_string()],
            }
        );
    }

    #[test]
    fn loads_manifest_marketplace_without_sparse() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[[claude.marketplaces]]
source = "jackin-project/jackin-marketplace"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.claude.as_ref().unwrap().marketplaces.len(), 1);
        assert_eq!(
            manifest.claude.as_ref().unwrap().marketplaces[0],
            ClaudeMarketplaceConfig {
                source: "jackin-project/jackin-marketplace".to_string(),
                sparse: vec![],
            }
        );
        assert!(manifest.claude.as_ref().unwrap().plugins.is_empty());
    }

    #[test]
    fn loads_manifest_without_plugins_defaults_to_empty() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert!(manifest.claude.as_ref().unwrap().plugins.is_empty());
        assert_eq!(manifest.claude.as_ref().unwrap().marketplaces.len(), 1);
    }

    #[test]
    fn loads_manifest_with_identity() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.identity.as_ref().unwrap().name, "Agent Smith");
    }

    #[test]
    fn display_name_uses_identity_when_present() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "Agent Smith");
    }

    #[test]
    fn display_name_falls_back_to_class_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "agent-smith");
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
unknown_field = true

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_claude_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
typo = "oops"
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_identity_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Smith"
typo = true

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn loads_manifest_with_hooks() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(
            manifest.hooks.as_ref().unwrap().pre_launch.as_deref(),
            Some("hooks/pre-launch.sh")
        );
    }

    #[test]
    fn loads_manifest_without_hooks() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert!(manifest.hooks.is_none());
    }

    #[test]
    fn rejects_unknown_hooks_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
post_launch = "bad"
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn loads_manifest_with_static_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.RUNTIME]
default = "docker"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.env.len(), 1);
        let var = &manifest.env["RUNTIME"];
        assert_eq!(var.default_value.as_deref(), Some("docker"));
        assert!(!var.interactive);
    }

    #[test]
    fn loads_manifest_with_interactive_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Select a project:"
options = ["project1", "project2"]
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        let var = &manifest.env["PROJECT"];
        assert!(var.interactive);
        assert_eq!(var.prompt.as_deref(), Some("Select a project:"));
        assert_eq!(var.options, vec!["project1", "project2"]);
    }

    #[test]
    fn loads_manifest_with_env_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Select:"
options = ["a", "b"]

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        let var = &manifest.env["BRANCH"];
        assert_eq!(var.depends_on, vec!["env.PROJECT"]);
    }

    #[test]
    fn loads_manifest_with_skippable_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.API_KEY]
interactive = true
skippable = true
prompt = "API key (optional):"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        let var = &manifest.env["API_KEY"];
        assert!(var.skippable);
    }

    #[test]
    fn loads_manifest_without_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert!(manifest.env.is_empty());
    }

    #[test]
    fn rejects_unknown_env_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
typo = true
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }
}
