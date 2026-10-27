// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::EnvCycleError;
use jackin_core::EnvVarDecl;
use std::collections::BTreeMap;

pub use jackin_core::PromptResult;

#[derive(Debug, thiserror::Error)]
pub enum ResolveEnvError {
    #[error("env var {name}: required prompt cannot be skipped")]
    PromptRequired { name: String },

    #[error(transparent)]
    Cycle(#[from] EnvCycleError),
}

#[derive(Debug, Clone)]
pub struct ResolvedEnv {
    pub vars: Vec<(String, String)>,
}

pub trait EnvPrompter {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult>;
    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult>;
}

fn interpolate(template: &str, resolved: &[(String, String)]) -> String {
    let resolved_map: std::collections::HashMap<&str, &str> = resolved
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut result = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find('}') {
            let ref_expr = &after_open[..end];
            if let Some(var_name) = ref_expr.strip_prefix("env.") {
                if let Some(&value) = resolved_map.get(var_name) {
                    result.push_str(value);
                } else {
                    result.push_str(&rest[start..=start + 2 + end]);
                }
            } else {
                result.push_str(&rest[start..=start + 2 + end]);
            }
            rest = &after_open[end + 1..];
        } else {
            result.push_str(&rest[start..]);
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result
}

pub fn resolve_env(
    declarations: &BTreeMap<String, EnvVarDecl>,
    prompter: &impl EnvPrompter,
) -> anyhow::Result<ResolvedEnv> {
    resolve_env_with_overrides(declarations, prompter, &BTreeMap::new())
}

pub fn resolve_env_with_overrides(
    declarations: &BTreeMap<String, EnvVarDecl>,
    prompter: &impl EnvPrompter,
    overrides: &BTreeMap<String, String>,
) -> anyhow::Result<ResolvedEnv> {
    let order = jackin_core::topological_env_order(declarations).map_err(ResolveEnvError::from)?;
    let mut vars = Vec::new();
    let mut skipped: std::collections::HashSet<String> = std::collections::HashSet::new();

    for name in &order {
        let decl = &declarations[name];

        if let Some(value) = overrides.get(name) {
            vars.push((name.clone(), value.clone()));
            continue;
        }

        let dep_skipped = decl.depends_on.iter().any(|dep| {
            dep.strip_prefix("env.")
                .is_some_and(|dep_name| skipped.contains(dep_name))
        });

        if dep_skipped {
            skipped.insert(name.clone());
            continue;
        }

        let interpolated_default = decl.default_value.as_deref().map(|d| interpolate(d, &vars));

        if !decl.interactive {
            if let Some(default) = interpolated_default {
                vars.push((name.clone(), default));
            }
            continue;
        }

        let raw_title = decl.prompt.as_deref().unwrap_or(name.as_str());
        let title = interpolate(raw_title, &vars);

        let result = if decl.options.is_empty() {
            prompter.prompt_text(&title, interpolated_default.as_deref(), decl.skippable)
        } else {
            prompter.prompt_select(
                &title,
                &decl.options,
                interpolated_default.as_deref(),
                decl.skippable,
            )
        }?;

        match result {
            PromptResult::Value(value) => {
                vars.push((name.clone(), value));
            }
            PromptResult::Skipped => {
                if decl.skippable {
                    skipped.insert(name.clone());
                } else {
                    return Err(anyhow::Error::new(ResolveEnvError::PromptRequired {
                        name: name.clone(),
                    }));
                }
            }
        }
    }

    Ok(ResolvedEnv { vars })
}
