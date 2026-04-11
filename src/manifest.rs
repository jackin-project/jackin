use crate::env_resolver::extract_interpolation_refs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const JACKIN_RUNTIME_ENV_NAME: &str = "JACKIN_CLAUDE_ENV";
pub const JACKIN_RUNTIME_ENV_VALUE: &str = "jackin";
pub const JACKIN_DIND_HOSTNAME_ENV_NAME: &str = "JACKIN_DIND_HOSTNAME";

const RESERVED_RUNTIME_ENV_VARS: &[(&str, Option<&str>)] = &[
    (JACKIN_RUNTIME_ENV_NAME, Some(JACKIN_RUNTIME_ENV_VALUE)),
    (JACKIN_DIND_HOSTNAME_ENV_NAME, None),
    // Docker TLS vars injected by jackin — must not be overridden by manifests.
    ("DOCKER_HOST", None),
    ("DOCKER_TLS_VERIFY", None),
    ("DOCKER_CERT_PATH", None),
];

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

/// Check that an env var name contains only `[A-Za-z0-9_]` and doesn't start with a digit.
fn is_valid_env_var_name(name: &str) -> bool {
    !name.is_empty()
        && name.is_ascii()
        && !name.as_bytes()[0].is_ascii_digit()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
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

    pub fn validate(&self) -> anyhow::Result<Vec<ManifestWarning>> {
        let mut warnings = Vec::new();

        for (name, decl) in &self.env {
            // Env var names must be valid identifiers: [A-Za-z_][A-Za-z0-9_]*
            if !is_valid_env_var_name(name) {
                anyhow::bail!(
                    "env var \"{name}\": name must contain only ASCII letters, digits, and underscores, and cannot start with a digit"
                );
            }

            if let Some((_, value)) = RESERVED_RUNTIME_ENV_VARS
                .iter()
                .find(|(reserved, _)| name == reserved)
            {
                let detail = value.as_ref().map_or_else(
                    || " and set automatically by jackin at runtime".to_string(),
                    |value| format!(" and set automatically to {value}"),
                );
                anyhow::bail!("env var {name}: reserved for jackin runtime metadata{detail}");
            }

            // Non-interactive without default is an error
            if !decl.interactive && decl.default_value.is_none() {
                anyhow::bail!("env var {name}: non-interactive variable must have a default value");
            }

            // options without interactive is an error
            if !decl.interactive && !decl.options.is_empty() {
                anyhow::bail!("env var {name}: options requires interactive = true");
            }

            // prompt without interactive is a warning
            if !decl.interactive && decl.prompt.is_some() {
                warnings.push(ManifestWarning {
                    message: format!(
                        "env var {name}: prompt is ignored without interactive = true"
                    ),
                });
            }

            // skippable without interactive is a warning
            if !decl.interactive && decl.skippable {
                warnings.push(ManifestWarning {
                    message: format!(
                        "env var {name}: skippable is meaningless without interactive = true"
                    ),
                });
            }

            self.validate_env_interpolation(name, decl)?;
        }

        // Cycle detection via topological sort (Kahn's algorithm)
        self.detect_env_cycles()?;

        Ok(warnings)
    }

    /// Validate a single `env.VAR_NAME` reference: non-empty, valid name, and declared.
    fn validate_env_ref(&self, owner: &str, context: &str, ref_name: &str) -> anyhow::Result<()> {
        if ref_name.is_empty() {
            anyhow::bail!("env var {owner}: {context} contains empty env reference \"env.\"");
        }
        if !is_valid_env_var_name(ref_name) {
            anyhow::bail!(
                "env var {owner}: {context} contains invalid env var name \"{ref_name}\""
            );
        }
        if !self.env.contains_key(ref_name) {
            anyhow::bail!("env var {owner}: {context} references unknown env var \"{ref_name}\"");
        }
        Ok(())
    }

    fn validate_env_interpolation(&self, name: &str, decl: &EnvVarDecl) -> anyhow::Result<()> {
        // Reject ${env.*} interpolation placeholders in options (options are always static)
        for option in &decl.options {
            if !extract_interpolation_refs(option).is_empty() {
                anyhow::bail!(
                    "env var {name}: options cannot contain interpolation placeholders — options are static"
                );
            }
        }

        // Validate depends_on entries
        let mut seen_deps: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for dep in &decl.depends_on {
            let Some(dep_name) = dep.strip_prefix("env.") else {
                anyhow::bail!(
                    "env var {name}: depends_on entry \"{dep}\" must use env. prefix (e.g., \"env.{dep}\")"
                );
            };

            if dep_name == name {
                anyhow::bail!("env var {name}: depends_on cannot reference self");
            }

            if !seen_deps.insert(dep_name) {
                anyhow::bail!("env var {name}: depends_on contains duplicate entry \"{dep_name}\"");
            }

            self.validate_env_ref(name, "depends_on", dep_name)?;
        }

        // Validate ${env.VAR_NAME} interpolation references in prompt and default_value
        let dep_names: std::collections::HashSet<&str> = decl
            .depends_on
            .iter()
            .filter_map(|d| d.strip_prefix("env."))
            .collect();

        for (field, value) in [
            ("prompt", decl.prompt.as_deref()),
            ("default", decl.default_value.as_deref()),
        ] {
            if let Some(v) = value {
                for ref_name in extract_interpolation_refs(v) {
                    self.validate_env_ref(name, field, ref_name)?;
                    if !dep_names.contains(ref_name) {
                        anyhow::bail!(
                            "env var {name}: {field} references \"{ref_name}\" which is not listed in depends_on"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn detect_env_cycles(&self) -> anyhow::Result<()> {
        use std::collections::{HashMap, VecDeque};

        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        for name in self.env.keys() {
            in_degree.entry(name.as_str()).or_insert(0);
            adjacency.entry(name.as_str()).or_default();
        }

        for (name, decl) in &self.env {
            for dep in &decl.depends_on {
                if let Some(dep_name) = dep.strip_prefix("env.") {
                    adjacency.entry(dep_name).or_default().push(name.as_str());
                    *in_degree.entry(name.as_str()).or_insert(0) += 1;
                }
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        let mut visited = 0usize;

        while let Some(node) = queue.pop_front() {
            visited += 1;
            if let Some(neighbors) = adjacency.get(node) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        if visited != self.env.len() {
            anyhow::bail!("env var dependency cycle detected");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn is_valid_env_var_name_accepts_standard_names() {
        assert!(is_valid_env_var_name("FOO"));
        assert!(is_valid_env_var_name("_PRIVATE"));
        assert!(is_valid_env_var_name("FOO_BAR_123"));
        assert!(is_valid_env_var_name("mixedCase"));
    }

    #[test]
    fn is_valid_env_var_name_rejects_invalid_names() {
        assert!(!is_valid_env_var_name(""));
        assert!(!is_valid_env_var_name("1FOO"));
        assert!(!is_valid_env_var_name("MY-VAR"));
        assert!(!is_valid_env_var_name("MY.VAR"));
        assert!(!is_valid_env_var_name("MY$VAR"));
        assert!(!is_valid_env_var_name("A}B"));
    }

    #[test]
    fn loads_manifest_with_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.dockerfile, "Dockerfile");
        assert!(manifest.claude.marketplaces.is_empty());
        assert_eq!(manifest.claude.plugins.len(), 1);
        assert!(manifest.identity.is_none());
    }

    #[test]
    fn loads_manifest_with_marketplaces_and_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(
            manifest.claude.plugins,
            vec!["superpowers@superpowers-marketplace"]
        );
        assert_eq!(manifest.claude.marketplaces.len(), 1);
        assert_eq!(
            manifest.claude.marketplaces[0],
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
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[[claude.marketplaces]]
source = "jackin-project/jackin-marketplace"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.claude.marketplaces.len(), 1);
        assert_eq!(
            manifest.claude.marketplaces[0],
            ClaudeMarketplaceConfig {
                source: "jackin-project/jackin-marketplace".to_string(),
                sparse: vec![],
            }
        );
        assert!(manifest.claude.plugins.is_empty());
    }

    #[test]
    fn loads_manifest_without_plugins_defaults_to_empty() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert!(manifest.claude.plugins.is_empty());
        assert_eq!(manifest.claude.marketplaces.len(), 1);
    }

    #[test]
    fn loads_manifest_with_identity() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"
unknown_field = true

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
typo = "oops"
"#,
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
            r#"dockerfile = "Dockerfile"

[identity]
name = "Smith"
typo = true

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
pre_launch = "hooks/pre-launch.sh"
post_launch = "bad"
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.RUNTIME]
default = "docker"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.env.len(), 1);
        let var = &manifest.env["RUNTIME"];
        assert_eq!(var.default_value.as_deref(), Some("docker"));
        assert!(!var.interactive);
    }

    #[test]
    fn loads_manifest_with_interactive_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
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

        let manifest = AgentManifest::load(temp.path()).unwrap();

        let var = &manifest.env["BRANCH"];
        assert_eq!(var.depends_on, vec!["env.PROJECT"]);
    }

    #[test]
    fn loads_manifest_with_skippable_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
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

        let manifest = AgentManifest::load(temp.path()).unwrap();

        let var = &manifest.env["API_KEY"];
        assert!(var.skippable);
    }

    #[test]
    fn loads_manifest_without_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
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
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
typo = true
"#,
        )
        .unwrap();

        let error = AgentManifest::load(temp.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn validate_rejects_non_interactive_without_default() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FOO"));
    }

    #[test]
    fn validate_rejects_options_without_interactive() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
options = ["a", "b"]
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("options"));
    }

    #[test]
    fn validate_rejects_dangling_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = ["env.NONEXISTENT"]
prompt = "Branch:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
    }

    #[test]
    fn validate_rejects_self_referencing_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env.FOO"]
prompt = "Value:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("self"));
    }

    #[test]
    fn validate_rejects_dependency_cycle() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.A]
interactive = true
depends_on = ["env.B"]
prompt = "A:"

[env.B]
interactive = true
depends_on = ["env.A"]
prompt = "B:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn validate_rejects_depends_on_without_env_prefix() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Project:"

[env.BRANCH]
interactive = true
depends_on = ["PROJECT"]
prompt = "Branch:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("env."));
    }

    #[test]
    fn validate_accepts_valid_manifest_with_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.RUNTIME]
default = "docker"

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Pick:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
default = "main"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_rejects_reserved_claude_env_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN_CLAUDE_ENV]
default = "docker"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("JACKIN_CLAUDE_ENV")
        );
    }

    #[test]
    fn validate_rejects_reserved_dind_hostname_env_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN_DIND_HOSTNAME]
default = "sidecar"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("JACKIN_DIND_HOSTNAME")
        );
    }

    #[test]
    fn validate_rejects_reserved_docker_tls_env_vars() {
        for var in ["DOCKER_HOST", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"] {
            let temp = tempdir().unwrap();
            std::fs::write(
                temp.path().join("jackin.agent.toml"),
                format!(
                    r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.{var}]
default = "override"
"#
                ),
            )
            .unwrap();

            let manifest = AgentManifest::load(temp.path()).unwrap();
            let result = manifest.validate();

            assert!(result.is_err(), "{var} should be rejected as reserved");
            assert!(
                result.unwrap_err().to_string().contains(var),
                "error message should mention {var}"
            );
        }
    }

    #[test]
    fn validate_warns_on_prompt_without_interactive() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
prompt = "This is ignored"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(!warnings.is_empty());
        assert!(warnings[0].message.contains("prompt"));
    }

    #[test]
    fn validate_warns_on_skippable_without_interactive() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
skippable = true
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(!warnings.is_empty());
        assert!(warnings[0].message.contains("skippable"));
    }

    #[test]
    fn validate_accepts_interpolation_in_prompt_and_default() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["project1", "project2"]
prompt = "Select a project:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch name for ${env.PROJECT}:"
default = "feature/${env.PROJECT}"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_rejects_interpolation_referencing_unknown_var() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = []
prompt = "Branch for ${env.NONEXISTENT}:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
    }

    #[test]
    fn validate_rejects_interpolation_not_in_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Select:"

[env.BRANCH]
interactive = true
prompt = "Branch for ${env.PROJECT}:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("PROJECT"));
        assert!(msg.contains("depends_on"));
    }

    #[test]
    fn validate_rejects_interpolation_in_default_referencing_unknown_var() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = []
default = "feature/${env.GHOST}"
prompt = "Branch:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GHOST"));
    }

    #[test]
    fn validate_rejects_invalid_env_var_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env."MY-VAR"]
default = "value"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("MY-VAR"));
    }

    #[test]
    fn validate_rejects_env_var_name_starting_with_digit() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env."1FOO"]
default = "value"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1FOO"));
    }

    #[test]
    fn validate_accepts_valid_env_var_names() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env._PRIVATE]
default = "a"

[env.UPPER_CASE_123]
default = "b"

[env.mixedCase]
default = "c"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_interpolation_in_options() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Pick:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
options = ["${env.PROJECT}-main", "${env.PROJECT}-dev"]
prompt = "Branch:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("options cannot contain interpolation")
        );
    }

    #[test]
    fn validate_ignores_non_env_namespace_in_interpolation() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value (use ${other.THING} for other):"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        // ${other.THING} is not an env. ref, so no error or warning
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_rejects_interpolation_in_default_not_in_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Select:"

[env.BRANCH]
interactive = true
prompt = "Branch:"
default = "feature/${env.PROJECT}"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("PROJECT"));
        assert!(msg.contains("depends_on"));
    }

    #[test]
    fn validate_rejects_when_one_of_multiple_refs_is_invalid() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Select:"

[env.LABEL]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Label for ${env.PROJECT} in ${env.MISSING}:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("MISSING"));
    }

    #[test]
    fn validate_rejects_empty_env_ref_in_prompt() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value: ${env.}"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_invalid_var_name_in_interpolation_ref() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = []
prompt = "Value: ${env.MY-VAR}"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid env var name")
        );
    }

    #[test]
    fn validate_rejects_empty_env_ref_in_default() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value:"
default = "prefix-${env.}"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_empty_depends_on_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env."]
prompt = "Value:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_invalid_depends_on_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env.MY-VAR"]
prompt = "Value:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid env var name")
        );
    }

    #[test]
    fn validate_rejects_duplicate_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Select:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT", "env.PROJECT"]
prompt = "Branch:"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicate"));
    }
}
