// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const JACKIN_ENV_NAME: &str = "JACKIN";

pub const JACKIN_ENV_VALUE: &str = "1";
pub const JACKIN_DIND_HOSTNAME_ENV_NAME: &str = "JACKIN_DIND_HOSTNAME";
pub const TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME: &str = "TESTCONTAINERS_HOST_OVERRIDE";
pub const JACKIN_CONTAINER_NAME_ENV_NAME: &str = "JACKIN_CONTAINER_NAME";
pub const JACKIN_INSTANCE_ID_ENV_NAME: &str = "JACKIN_INSTANCE_ID";
pub const JACKIN_CAPSULE_SUPERVISOR_PID_ENV_NAME: &str = "JACKIN_CAPSULE_SUPERVISOR_PID";
pub const JACKIN_AGENT_ENV_NAME: &str = "JACKIN_AGENT";
pub const JACKIN_AGENT_CODENAME_ENV_NAME: &str = "JACKIN_AGENT_CODENAME";
pub const JACKIN_ROLE_ENV_NAME: &str = "JACKIN_ROLE";
pub const JACKIN_WORKDIR_ENV_NAME: &str = "JACKIN_WORKDIR";
pub const JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME: &str = "JACKIN_GIT_COAUTHOR_TRAILER";
pub const JACKIN_GIT_DCO_ENV_NAME: &str = "JACKIN_GIT_DCO";
pub const JACKIN_OPEN_LINKS_ENV_NAME: &str = "JACKIN_OPEN_LINKS";
pub const CODEX_LANE_MODEL_ENV_NAME: &str = "JACKIN_LANE_CODEX_MODEL";
pub const CODEX_LANE_EFFORT_ENV_NAME: &str = "JACKIN_LANE_CODEX_EFFORT";
pub const CLAUDE_MODEL_ENV_NAME: &str = "ANTHROPIC_MODEL";
pub const CLAUDE_EFFORT_ENV_NAME: &str = "CLAUDE_CODE_EFFORT_LEVEL";
pub const ZAI_API_KEY_ENV_NAME: &str = "ZAI_API_KEY";
pub const ZHIPU_API_KEY_ENV_NAME: &str = "ZHIPU_API_KEY";
pub const ANTHROPIC_API_KEY_ENV_NAME: &str = "ANTHROPIC_API_KEY";
pub const ANTHROPIC_AUTH_TOKEN_ENV_NAME: &str = "ANTHROPIC_AUTH_TOKEN";
pub const OPENAI_API_KEY_ENV_NAME: &str = "OPENAI_API_KEY";
pub const AMP_API_KEY_ENV_NAME: &str = "AMP_API_KEY";
pub const CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME: &str = "CLAUDE_CODE_OAUTH_TOKEN";
pub const MINIMAX_API_KEY_ENV_NAME: &str = "MINIMAX_API_KEY";
pub const KIMI_CODE_API_KEY_ENV_NAME: &str = "KIMI_CODE_API_KEY";
pub const KIMI_API_KEY_ENV_NAME: &str = "KIMI_API_KEY";
pub const MOONSHOT_API_KEY_ENV_NAME: &str = "MOONSHOT_API_KEY";
pub const OPENCODE_API_KEY_ENV_NAME: &str = "OPENCODE_API_KEY";
pub const XAI_API_KEY_ENV_NAME: &str = "XAI_API_KEY";
pub const GEMINI_API_KEY_ENV_NAME: &str = "GEMINI_API_KEY";
pub const GOOGLE_API_KEY_ENV_NAME: &str = "GOOGLE_API_KEY";
pub const CURSOR_API_KEY_ENV_NAME: &str = "CURSOR_API_KEY";
pub const META_API_KEY_ENV_NAME: &str = "META_API_KEY";
pub const OPENROUTER_API_KEY_ENV_NAME: &str = "OPENROUTER_API_KEY";
pub const GROK_DEPLOYMENT_KEY_ENV_NAME: &str = "GROK_DEPLOYMENT_KEY";
pub const GH_TOKEN_ENV_NAME: &str = "GH_TOKEN";
pub const GITHUB_TOKEN_ENV_NAME: &str = "GITHUB_TOKEN";
pub const GH_HOST_ENV_NAME: &str = "GH_HOST";
pub const GH_ENTERPRISE_TOKEN_ENV_NAME: &str = "GH_ENTERPRISE_TOKEN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UsageCredentialOwner {
    Claude,
    Codex,
    Amp,
    Kimi,
    Grok,
    Zai,
    Minimax,
    OpenCode,
    Google,
    Cursor,
    Meta,
    OpenRouter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageCredentialEnvName {
    pub name: &'static str,
    pub owner: UsageCredentialOwner,
}

pub const USAGE_CREDENTIAL_ENV_REGISTRY: &[UsageCredentialEnvName] = &[
    UsageCredentialEnvName {
        name: ANTHROPIC_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Claude,
    },
    UsageCredentialEnvName {
        name: ANTHROPIC_AUTH_TOKEN_ENV_NAME,
        owner: UsageCredentialOwner::Claude,
    },
    UsageCredentialEnvName {
        name: CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
        owner: UsageCredentialOwner::Claude,
    },
    UsageCredentialEnvName {
        name: OPENAI_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Codex,
    },
    UsageCredentialEnvName {
        name: AMP_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Amp,
    },
    UsageCredentialEnvName {
        name: KIMI_CODE_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Kimi,
    },
    UsageCredentialEnvName {
        name: KIMI_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Kimi,
    },
    UsageCredentialEnvName {
        name: MOONSHOT_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Kimi,
    },
    UsageCredentialEnvName {
        name: XAI_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Grok,
    },
    UsageCredentialEnvName {
        name: GROK_DEPLOYMENT_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Grok,
    },
    UsageCredentialEnvName {
        name: ZAI_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Zai,
    },
    UsageCredentialEnvName {
        name: ZHIPU_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Zai,
    },
    UsageCredentialEnvName {
        name: MINIMAX_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Minimax,
    },
    UsageCredentialEnvName {
        name: OPENCODE_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::OpenCode,
    },
    UsageCredentialEnvName {
        name: GEMINI_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Google,
    },
    UsageCredentialEnvName {
        name: GOOGLE_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Google,
    },
    UsageCredentialEnvName {
        name: CURSOR_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Cursor,
    },
    UsageCredentialEnvName {
        name: META_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::Meta,
    },
    UsageCredentialEnvName {
        name: OPENROUTER_API_KEY_ENV_NAME,
        owner: UsageCredentialOwner::OpenRouter,
    },
];

pub const JACKIN_NETWORK_MODE_ENV_NAME: &str = "JACKIN_NETWORK_MODE";
pub const JACKIN_ALLOWED_HOSTS_ENV_NAME: &str = "JACKIN_ALLOWED_HOSTS";
pub const JACKIN_FIREWALL_INSTALLED_ENV_NAME: &str = "JACKIN_FIREWALL_INSTALLED";
pub const JACKIN_NETWORK_ENFORCEMENT_ENV_NAME: &str = "JACKIN_NETWORK_ENFORCEMENT";
pub const JACKIN_SUDO_ENV_NAME: &str = "JACKIN_SUDO";

pub const RESERVED_RUNTIME_ENV_VARS: &[(&str, Option<&str>)] = &[
    (JACKIN_ENV_NAME, Some(JACKIN_ENV_VALUE)),
    (JACKIN_DIND_HOSTNAME_ENV_NAME, None),
    (JACKIN_CONTAINER_NAME_ENV_NAME, None),
    (JACKIN_INSTANCE_ID_ENV_NAME, None),
    (JACKIN_CAPSULE_SUPERVISOR_PID_ENV_NAME, None),
    (JACKIN_AGENT_ENV_NAME, None),
    (JACKIN_AGENT_CODENAME_ENV_NAME, None),
    (JACKIN_ROLE_ENV_NAME, None),
    (JACKIN_WORKDIR_ENV_NAME, None),
    (JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, None),
    (JACKIN_GIT_DCO_ENV_NAME, None),
    ("DOCKER_HOST", None),
    ("DOCKER_TLS_VERIFY", None),
    ("DOCKER_CERT_PATH", None),
    (TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME, None),
    (JACKIN_NETWORK_MODE_ENV_NAME, None),
    (JACKIN_ALLOWED_HOSTS_ENV_NAME, None),
    (JACKIN_FIREWALL_INSTALLED_ENV_NAME, None),
    (JACKIN_NETWORK_ENFORCEMENT_ENV_NAME, None),
    (JACKIN_SUDO_ENV_NAME, None),
    (CODEX_LANE_MODEL_ENV_NAME, None),
    (CODEX_LANE_EFFORT_ENV_NAME, None),
    (CLAUDE_MODEL_ENV_NAME, None),
    (CLAUDE_EFFORT_ENV_NAME, None),
];

pub fn is_reserved(name: &str) -> bool {
    RESERVED_RUNTIME_ENV_VARS
        .iter()
        .any(|(reserved, _)| *reserved == name)
}

pub fn env_value_is_deny(value: &str) -> bool {
    matches!(value, "deny" | "off" | "no")
}

pub fn open_links_allowed(value: Option<&str>) -> bool {
    value.is_none_or(|value| !env_value_is_deny(value))
}

pub fn extract_interpolation_refs(s: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        let Some(after_open) = rest.get(start + 2..) else {
            break;
        };
        if let Some(end) = after_open.find('}') {
            let Some(ref_expr) = after_open.get(..end) else {
                break;
            };
            if let Some(var_name) = ref_expr.strip_prefix("env.") {
                refs.push(var_name);
            }
            let Some(next) = after_open.get(end + 1..) else {
                break;
            };
            rest = next;
        } else {
            break;
        }
    }
    refs
}

#[derive(Debug)]
pub struct EnvCycleError;

impl std::fmt::Display for EnvCycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("env var dependency cycle detected")
    }
}

impl std::error::Error for EnvCycleError {}

#[expect(
    clippy::excessive_nesting,
    reason = "Kahn's algorithm topological-sort body: the read-side / \
              decrement-degree / enqueue-ready nesting is the canonical \
              topological-sort structure. Extracting into helper fns would \
              require re-passing the in-degree / adjacency / ready mutable \
              borrows and obscure the algorithm."
)]
pub fn topological_env_order(
    declarations: &std::collections::BTreeMap<String, crate::manifest::EnvVarDecl>,
) -> Result<Vec<String>, EnvCycleError> {
    use std::collections::{BTreeSet, HashMap};

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in declarations.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
        adjacency.entry(name.as_str()).or_default();
    }

    for (name, decl) in declarations {
        for dep in &decl.depends_on {
            if let Some(dep_name) = dep.strip_prefix("env.") {
                adjacency.entry(dep_name).or_default().push(name.as_str());
                *in_degree.entry(name.as_str()).or_insert(0) += 1;
            }
        }
    }

    let mut ready: BTreeSet<&str> = in_degree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut result = Vec::new();

    while let Some(node) = ready.pop_first() {
        result.push(node.to_owned());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.insert(neighbor);
                    }
                }
            }
        }
    }

    if result.len() != declarations.len() {
        return Err(EnvCycleError);
    }

    Ok(result)
}

#[cfg(test)]
mod tests;
