pub use crate::env_model::{JACKIN_DIND_HOSTNAME_ENV_NAME, JACKIN_ENV_NAME, JACKIN_ENV_VALUE};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod migrations;
pub mod validate;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleManifest {
    #[serde(default = "migrations::current_manifest_version", rename = "version")]
    pub version: String,
    pub dockerfile: String,
    /// Pre-built Docker image published to a registry. When set, `jackin
    /// console` pulls this image and layers only the agent install on top,
    /// skipping the full workspace Dockerfile build. Pass `--rebuild` to
    /// force a local rebuild from the Dockerfile instead.
    #[serde(default)]
    pub published_image: Option<String>,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    /// Top-level list of supported agents. `None` means the field
    /// was omitted, which `supported_agents()` treats as
    /// claude-only (the implicit default). `Some(empty)` is
    /// rejected by validate as a user error.
    #[serde(default)]
    pub agents: Option<Vec<crate::agent::Agent>>,
    #[serde(default)]
    pub claude: Option<ClaudeConfig>,
    #[serde(default)]
    pub codex: Option<CodexConfig>,
    #[serde(default)]
    pub amp: Option<AmpConfig>,
    #[serde(default)]
    pub opencode: Option<OpencodeConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    /// Optional model override; passed to Codex with `-m` when present,
    /// otherwise Codex's own default is used.
    #[serde(default)]
    pub model: Option<String>,
}

/// Per-role Amp configuration.
///
/// Has no fields. Declared so manifests that list
/// `agents = [..., "amp"]` can carry an `[amp]` table that satisfies
/// the agent/table consistency check.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmpConfig {}

/// Per-role `OpenCode` configuration.
///
/// `model` is passed to `OpenCode` with `-m` in `provider/model` format
/// (e.g. `zai-coding-plan/glm-5.1`). When absent, `OpenCode` uses its
/// own default model selection.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpencodeConfig {
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    #[serde(default)]
    pub setup_once: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub preflight: Option<String>,
}

/// Centralizes the (label, in-image filename, repo-relative path) triple
/// so repo validation, Dockerfile rendering, and `.dockerignore`
/// allowlisting cannot disagree about a hook's identity.
#[derive(Debug, Clone, Copy)]
pub struct HookEntry<'a> {
    pub(crate) label: &'static str,
    pub(crate) filename: &'static str,
    pub(crate) path: &'a str,
}

impl HooksConfig {
    pub fn entries(&self) -> impl Iterator<Item = HookEntry<'_>> {
        // Order is the entrypoint.sh runtime contract; pinned by
        // `hook_entries_yield_runtime_contract_order`.
        [
            (
                "setup_once hook",
                "setup-once.sh",
                self.setup_once.as_deref(),
            ),
            ("source hook", "source.sh", self.source.as_deref()),
            ("preflight hook", "preflight.sh", self.preflight.as_deref()),
        ]
        .into_iter()
        .filter_map(|(label, filename, path)| {
            path.map(|path| HookEntry {
                label,
                filename,
                path,
            })
        })
    }
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
    /// Optional model override; passed to Claude Code with `--model`
    /// when present, otherwise Claude Code's own default is used.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub marketplaces: Vec<ClaudeMarketplaceConfig>,
    #[serde(default)]
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ManifestWarning {
    pub message: String,
}

impl ManifestWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl RoleManifest {
    /// Parse `jackin.role.toml` and pin the load-time invariants other code
    /// relies on: `version` is not newer than this binary understands,
    /// version-gated features match the manifest's declared minimum schema,
    /// and the `agents` / `[<agent>]` tables are consistent (so
    /// `instance::prepare` can dereference `manifest.codex` /
    /// `manifest.claude` without re-checking).
    ///
    /// Env-var validation, interpolation cycle detection, and the rest of
    /// `validate()` still need explicit calls — they emit warnings the load
    /// path cannot surface.
    pub fn load(repo_dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = repo_dir.join("jackin.role.toml");
        let contents = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let doc: toml_edit::DocumentMut = contents
            .parse()
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let role_name = display_role_name(repo_dir);
        let manifest_version = crate::manifest::migrations::validate_manifest_version(&doc)
            .with_context(|| format!("validating version of {}", manifest_path.display()))?;
        let manifest: Self = toml::from_str(&contents)
            .with_context(|| format!("parsing {} as RoleManifest", manifest_path.display()))?;
        validate_feature_versions(&manifest, &manifest_version, &role_name)
            .with_context(|| format!("validating version of {}", manifest_path.display()))?;
        let _warnings = crate::manifest::validate::validate_agent_consistency(&manifest)?;
        Ok(manifest)
    }

    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_string(), |id| id.name.clone())
    }

    /// Returns the agents this manifest supports. Legacy manifests
    /// without an `agents` field default to claude-only.
    pub fn supported_agents(&self) -> Vec<crate::agent::Agent> {
        self.agents
            .clone()
            .unwrap_or_else(|| vec![crate::agent::Agent::Claude])
    }
}

fn display_role_name(repo_dir: &Path) -> String {
    let leaf = repo_dir.file_name().and_then(|name| name.to_str());
    let parent = repo_dir
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str());
    match (parent, leaf) {
        (Some(parent), Some("default" | "branches")) => parent.to_string(),
        (_, Some(name)) => name.to_string(),
        _ => repo_dir.display().to_string(),
    }
}

fn validate_feature_versions(
    manifest: &RoleManifest,
    manifest_version: &crate::config::migrations::SchemaVersion,
    role_name: &str,
) -> anyhow::Result<()> {
    let opencode_version = crate::config::migrations::parse_version("v1alpha3")?;
    if manifest_version < &opencode_version
        && (manifest
            .agents
            .as_ref()
            .is_some_and(|agents| agents.contains(&crate::agent::Agent::Opencode))
            || manifest.opencode.is_some())
    {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses opencode, which requires v1alpha3; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_manifest_with_agents_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp"]

[claude]
plugins = []

[codex]

[amp]
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(
            m.supported_agents(),
            vec![
                crate::agent::Agent::Claude,
                crate::agent::Agent::Codex,
                crate::agent::Agent::Amp
            ]
        );
        assert!(m.codex.is_some());
        assert!(m.amp.is_some());
    }

    #[test]
    fn legacy_manifest_without_agents_field_defaults_to_claude_only() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
model = "sonnet"
plugins = []
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(m.supported_agents(), vec![crate::agent::Agent::Claude]);
        assert_eq!(m.claude.as_ref().unwrap().model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn loads_codex_only_manifest() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
model = "gpt-5"
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(m.supported_agents(), vec![crate::agent::Agent::Codex]);
        assert_eq!(m.codex.as_ref().unwrap().model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn loads_opencode_manifest_with_model() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["opencode"]

[opencode]
model = "zai-coding-plan/glm-5.1"
"#,
        )
        .unwrap();

        let m = RoleManifest::load(temp.path()).unwrap();
        assert_eq!(m.supported_agents(), vec![crate::agent::Agent::Opencode]);
        assert_eq!(
            m.opencode.as_ref().unwrap().model.as_deref(),
            Some("zai-coding-plan/glm-5.1")
        );
    }

    #[test]
    fn rejects_unknown_agent_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "foo"]

[claude]
plugins = []
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        let chain = format!("{err:#}");
        assert!(
            chain.contains("foo") || chain.contains("unknown"),
            "{chain}"
        );
    }

    #[test]
    fn loads_unversioned_manifest_without_newer_features() {
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
        assert_eq!(
            manifest.supported_agents(),
            vec![crate::agent::Agent::Claude]
        );
    }

    #[test]
    fn rejects_newer_manifest_version() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v2alpha1"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("only understands up to v1alpha3"), "{chain}");
    }

    #[test]
    fn rejects_old_manifest_version_using_opencode_agent() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["opencode"]

[opencode]
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("requires v1alpha3"), "{chain}");
        assert!(chain.contains("jackin role migrate"), "{chain}");
    }

    #[test]
    fn rejects_old_manifest_version_with_opencode_table() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []

[opencode]
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("requires v1alpha3"), "{chain}");
    }

    #[test]
    fn loads_manifest_with_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
    fn display_name_falls_back_to_role_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.display_name("agent-smith"), "agent-smith");
    }

    #[test]
    fn loads_manifest_with_published_image() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert_eq!(
            manifest.published_image.as_deref(),
            Some("docker.io/myorg/my-role:latest")
        );
    }

    #[test]
    fn loads_manifest_without_published_image() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        assert!(manifest.published_image.is_none());
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
unknown_field = true

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(format!("{error:#}").contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_claude_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
typo = "oops"
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(format!("{error:#}").contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_identity_field() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[identity]
name = "Smith"
typo = true

[claude]
plugins = []
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(format!("{error:#}").contains("unknown field"));
    }

    #[test]
    fn hook_entries_yield_runtime_contract_order() {
        let hooks = HooksConfig {
            setup_once: Some("a.sh".to_string()),
            source: Some("b.sh".to_string()),
            preflight: Some("c.sh".to_string()),
        };
        let triples: Vec<_> = hooks
            .entries()
            .map(|e| (e.label, e.filename, e.path))
            .collect();
        assert_eq!(
            triples,
            [
                ("setup_once hook", "setup-once.sh", "a.sh"),
                ("source hook", "source.sh", "b.sh"),
                ("preflight hook", "preflight.sh", "c.sh"),
            ]
        );
    }

    #[test]
    fn hook_entries_skip_absent_and_preserve_order() {
        // Mixed presence: only source + preflight. Order must follow
        // the canonical sequence, not the order fields are populated.
        let hooks = HooksConfig {
            setup_once: None,
            source: Some("b.sh".to_string()),
            preflight: Some("c.sh".to_string()),
        };
        let labels: Vec<_> = hooks.entries().map(|e| e.label).collect();
        assert_eq!(labels, ["source hook", "preflight hook"]);
    }

    #[test]
    fn loads_manifest_with_hooks() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
setup_once = "hooks/setup-once.sh"
source = "hooks/source.sh"
preflight = "hooks/preflight.sh"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        let hooks = manifest.hooks.as_ref().unwrap();
        assert_eq!(hooks.setup_once.as_deref(), Some("hooks/setup-once.sh"));
        assert_eq!(hooks.source.as_deref(), Some("hooks/source.sh"));
        assert_eq!(hooks.preflight.as_deref(), Some("hooks/preflight.sh"));
    }

    #[test]
    fn loads_manifest_without_hooks() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
post_launch = "bad"
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(format!("{error:#}").contains("unknown field"));
    }

    #[test]
    fn loads_manifest_with_static_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

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
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
typo = true
"#,
        )
        .unwrap();

        let error = RoleManifest::load(temp.path()).unwrap_err();

        assert!(format!("{error:#}").contains("unknown field"));
    }
}
