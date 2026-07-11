//! Role manifest validation: checks consistency rules that serde alone cannot
//! express.
//!
//! `validate_role_manifest` returns hard errors for invalid manifests and
//! non-fatal `ManifestWarning` values for config that parses but is likely
//! wrong (e.g. agent tables present but not listed in `agents`).
//!
//! Not responsible for: parsing the manifest file (`manifest.rs`), or
//! validating the role-repo filesystem (`repo.rs`).

use jackin_core::env_model::extract_interpolation_refs;
use jackin_core::manifest::{EnvVarDecl, ManifestWarning, RoleManifest};

/// Check that an env var name contains only `[A-Za-z0-9_]` and doesn't start with a digit.
pub fn is_valid_env_var_name(name: &str) -> bool {
    !name.is_empty()
        && name.is_ascii()
        && name
            .as_bytes()
            .first()
            .is_some_and(|b| !b.is_ascii_digit())
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
/// - Without an `agents` field, the manifest is treated as
///   claude-only and must declare `[claude]`
///   (`supported_agents()` returns `[Claude]`).
///
/// Also surfaces an orphan-table warning (non-fatal): if any
/// `[<agent>]` table (`[claude]`, `[codex]`, `[amp]`) is populated
/// but the corresponding agent isn't listed in `agents`, the table
/// is dead config — every runtime path skips it. Authors editing
/// manifests by hand routinely add `[codex] model = "..."` first
/// and forget the `agents = [...]` declaration; without this signal
/// they'd have to debug "agent does not support codex" at load time
/// and figure out the connection themselves.
///
/// Orphan warnings are skipped on manifests with no `agents` field
/// since the implicit default is unambiguous: those manifests are
/// claude-only by definition, so a populated `[claude]` table is
/// expected and any other populated `[<agent>]` table would also
/// have failed the per-agent table-required check above.
pub fn validate_agent_consistency(manifest: &RoleManifest) -> anyhow::Result<Vec<ManifestWarning>> {
    use jackin_core::Agent;

    let supported = manifest.supported_agents();

    if let Some(list) = &manifest.agents
        && list.is_empty()
    {
        anyhow::bail!("`agents` must not be empty");
    }

    for h in &supported {
        if !manifest.has_agent_config(*h) {
            let slug = h.runtime().slug();
            anyhow::bail!("[{slug}] table required when {slug} is in `agents`");
        }
    }

    let mut warnings = Vec::new();

    // Only meaningful when `agents` is explicit — manifests with no
    // `agents` field implicitly default to claude-only and have their
    // own coverage rule above.
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
        if manifest.amp.is_some() && !supported.contains(&Agent::Amp) {
            warnings.push(ManifestWarning::new(
                "[amp] table is present but `agents` does not include amp; \
                 the table is ignored — add amp to `agents` to enable it.",
            ));
        }
        if manifest.kimi.is_some() && !supported.contains(&Agent::Kimi) {
            warnings.push(ManifestWarning::new(
                "[kimi] table is present but `agents` does not include kimi; \
                 the table is ignored — add kimi to `agents` to enable it.",
            ));
        }
        if manifest.opencode.is_some() && !supported.contains(&Agent::Opencode) {
            warnings.push(ManifestWarning::new(
                "[opencode] table is present but `agents` does not include opencode; \
                 the table is ignored — add opencode to `agents` to enable it.",
            ));
        }
    }

    Ok(warnings)
}

/// Validate env-var declarations and agent consistency.
///
/// Returns hard errors for invalid manifests and non-fatal `ManifestWarning`
/// values for config that parses but is likely wrong.
///
/// # Errors
/// Returns an error if the manifest has invalid env vars, reserved names,
/// or inconsistent agent configuration.
pub fn validate_role_manifest(manifest: &RoleManifest) -> anyhow::Result<Vec<ManifestWarning>> {
    let mut warnings = validate_agent_consistency(manifest)?;

    for (name, decl) in &manifest.env {
        // Env var names must be valid identifiers: [A-Za-z_][A-Za-z0-9_]*
        if !is_valid_env_var_name(name) {
            anyhow::bail!(
                "env var \"{name}\": name must contain only ASCII letters, digits, and underscores, and cannot start with a digit"
            );
        }

        if let Some((_, value)) = jackin_core::env_model::RESERVED_RUNTIME_ENV_VARS
            .iter()
            .find(|(reserved, _)| name == reserved)
        {
            let detail = value.as_ref().map_or_else(
                || " and set automatically by jackin at runtime".to_owned(),
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
                message: format!("env var {name}: prompt is ignored without interactive = true"),
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

        validate_env_interpolation(manifest, name, decl)?;
    }

    // Cycle detection via topological sort (shared with
    // env_resolver::resolve_env — one Kahn's implementation).
    jackin_core::env_model::topological_env_order(&manifest.env)?;

    Ok(warnings)
}

/// Validate a single `env.VAR_NAME` reference: non-empty, valid name, and declared.
fn validate_env_ref(
    manifest: &RoleManifest,
    owner: &str,
    context: &str,
    ref_name: &str,
) -> anyhow::Result<()> {
    if ref_name.is_empty() {
        anyhow::bail!("env var {owner}: {context} contains empty env reference \"env.\"");
    }
    if !is_valid_env_var_name(ref_name) {
        anyhow::bail!("env var {owner}: {context} contains invalid env var name \"{ref_name}\"");
    }
    if !manifest.env.contains_key(ref_name) {
        anyhow::bail!("env var {owner}: {context} references unknown env var \"{ref_name}\"");
    }
    Ok(())
}

fn validate_env_interpolation(
    manifest: &RoleManifest,
    name: &str,
    decl: &EnvVarDecl,
) -> anyhow::Result<()> {
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

        validate_env_ref(manifest, name, "depends_on", dep_name)?;
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
                validate_env_ref(manifest, name, field, ref_name)?;
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

#[cfg(test)]
mod tests;
