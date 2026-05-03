use super::{EnvVarDecl, ManifestWarning, RoleManifest};
use crate::env_model::extract_interpolation_refs;

/// Check that an env var name contains only `[A-Za-z0-9_]` and doesn't start with a digit.
pub(super) fn is_valid_env_var_name(name: &str) -> bool {
    !name.is_empty()
        && name.is_ascii()
        && !name.as_bytes()[0].is_ascii_digit()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Validate the `agents` / [<agent>] table consistency.
///
/// Rules enforced (hard errors):
/// - If `agents` is present, it must be non-empty.
/// - For every agent A in `agents`, the corresponding [A] table
///   must exist (even if empty), so consumers like
///   `instance/mod.rs::prepare` can rely on the table being non-`None`
///   when launching that agent without a runtime check.
/// - Without an `agents` field, the manifest must declare [claude]
///   (legacy default; `supported_agents()` returns `[Claude]`).
///
/// Also surfaces an orphan-table warning (non-fatal): if a `[claude]`
/// or `[codex]` table is populated but the corresponding agent isn't
/// listed in `agents`, the table is dead config — every runtime path
/// skips it. Authors editing manifests by hand routinely add
/// `[codex] model = "..."` first and forget the `agents = [...]`
/// declaration; without this signal they'd have to debug "agent does
/// not support codex" at load time and figure out the connection
/// themselves.
pub fn validate_agent_consistency(manifest: &RoleManifest) -> anyhow::Result<Vec<ManifestWarning>> {
    use crate::agent::Agent;

    let supported = manifest.supported_agents();

    if let Some(list) = &manifest.agents
        && list.is_empty()
    {
        anyhow::bail!("`agents` must not be empty");
    }

    for h in &supported {
        match h {
            Agent::Claude => {
                if manifest.claude.is_none() {
                    anyhow::bail!("[claude] table required when claude is in `agents`");
                }
            }
            Agent::Codex => {
                if manifest.codex.is_none() {
                    anyhow::bail!("[codex] table required when codex is in `agents`");
                }
            }
        }
    }

    let mut warnings = Vec::new();

    // Only meaningful when `agents` is explicit — legacy manifests
    // (no `agents` field) implicitly default to claude-only and have
    // their own coverage rule above.
    if manifest.agents.is_some() {
        if manifest.codex.is_some() && !supported.contains(&Agent::Codex) {
            warnings.push(ManifestWarning::new(
                "[codex] table is present but `agents` does not include codex; \
                 the table is ignored — add codex to `agents` to enable it.",
            ));
        }
        if manifest.claude.is_some() && !supported.contains(&Agent::Claude) {
            warnings.push(ManifestWarning::new(
                "[claude] table is present but `agents` does not include claude; \
                 the table is ignored — add claude to `agents` to enable it.",
            ));
        }
    }

    Ok(warnings)
}

impl RoleManifest {
    pub fn validate(&self) -> anyhow::Result<Vec<ManifestWarning>> {
        let mut warnings = validate_agent_consistency(self)?;

        for (name, decl) in &self.env {
            // Env var names must be valid identifiers: [A-Za-z_][A-Za-z0-9_]*
            if !is_valid_env_var_name(name) {
                anyhow::bail!(
                    "env var \"{name}\": name must contain only ASCII letters, digits, and underscores, and cannot start with a digit"
                );
            }

            if let Some((_, value)) = crate::env_model::RESERVED_RUNTIME_ENV_VARS
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

        // Cycle detection via topological sort (shared with
        // env_resolver::resolve_env — one Kahn's implementation).
        crate::env_model::topological_env_order(&self.env)?;

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
    fn rejects_empty_supported_list() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = []

[claude]
plugins = []
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn rejects_codex_supported_without_codex_table() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []
"#,
        )
        .unwrap();

        let err = RoleManifest::load(temp.path()).unwrap_err();
        assert!(err.to_string().contains("[codex]"));
    }

    #[test]
    fn legacy_manifest_with_claude_passes() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let warnings = RoleManifest::load(temp.path()).unwrap().validate().unwrap();
        assert!(warnings.is_empty());
    }

    /// Pin the orphan-codex-table warning: a manifest with `[codex]`
    /// populated but codex absent from `agents` is dead config, and
    /// the operator gets a warning that points at the fix instead of
    /// having to debug "agent does not support codex" at load time.
    #[test]
    fn warns_when_codex_table_present_without_codex_in_supported() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["claude"]

[claude]
plugins = []

[codex]
model = "gpt-5"
"#,
        )
        .unwrap();

        let warnings = RoleManifest::load(temp.path()).unwrap().validate().unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].message.contains("[codex]"));
        assert!(warnings[0].message.contains("ignored"));
    }

    /// Symmetric warning for the rare reverse case: `[claude]` populated
    /// but claude absent from `agents`.
    #[test]
    fn warns_when_claude_table_present_without_claude_in_supported() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["codex"]

[claude]
plugins = []

[codex]
"#,
        )
        .unwrap();

        let warnings = RoleManifest::load(temp.path()).unwrap().validate().unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].message.contains("[claude]"));
    }

    #[test]
    fn validate_rejects_non_interactive_without_default() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FOO"));
    }

    #[test]
    fn validate_rejects_options_without_interactive() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
options = ["a", "b"]
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("options"));
    }

    #[test]
    fn validate_rejects_dangling_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
    }

    #[test]
    fn validate_rejects_self_referencing_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("self"));
    }

    #[test]
    fn validate_rejects_dependency_cycle() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn validate_rejects_depends_on_without_env_prefix() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("env."));
    }

    #[test]
    fn validate_accepts_valid_manifest_with_env() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_rejects_reserved_claude_env_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN]
default = "docker"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JACKIN"));
    }

    #[test]
    fn validate_rejects_reserved_dind_hostname_env_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN_DIND_HOSTNAME]
default = "sidecar"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
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
                temp.path().join("jackin.role.toml"),
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

            let manifest = RoleManifest::load(temp.path()).unwrap();
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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
prompt = "This is ignored"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(!warnings.is_empty());
        assert!(warnings[0].message.contains("prompt"));
    }

    #[test]
    fn validate_warns_on_skippable_without_interactive() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
skippable = true
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(!warnings.is_empty());
        assert!(warnings[0].message.contains("skippable"));
    }

    #[test]
    fn validate_accepts_interpolation_in_prompt_and_default() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_rejects_interpolation_referencing_unknown_var() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
    }

    #[test]
    fn validate_rejects_interpolation_not_in_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
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
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GHOST"));
    }

    #[test]
    fn validate_rejects_invalid_env_var_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env."MY-VAR"]
default = "value"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("MY-VAR"));
    }

    #[test]
    fn validate_rejects_env_var_name_starting_with_digit() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env."1FOO"]
default = "value"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1FOO"));
    }

    #[test]
    fn validate_accepts_valid_env_var_names() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_interpolation_in_options() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
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
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value (use ${other.THING} for other):"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let warnings = manifest.validate().unwrap();

        // ${other.THING} is not an env. ref, so no error or warning
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_rejects_interpolation_in_default_not_in_depends_on() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
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
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("MISSING"));
    }

    #[test]
    fn validate_rejects_empty_env_ref_in_prompt() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value: ${env.}"
"#,
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_invalid_var_name_in_interpolation_ref() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
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
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_empty_depends_on_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_invalid_depends_on_name() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
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
            temp.path().join("jackin.role.toml"),
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

        let manifest = RoleManifest::load(temp.path()).unwrap();
        let result = manifest.validate();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicate"));
    }
}
