//! Env policy model: reserved runtime env vars and interpolation parsing.
//!
//! Single source of truth for:
//! * Names and default values of runtime-reserved environment variables.
//! * `${env.VAR_NAME}` interpolation reference parsing.

/// Env var injected by jackin into every role container.
pub const JACKIN_ENV_NAME: &str = "JACKIN";
/// Value for [`JACKIN_ENV_NAME`].
pub const JACKIN_ENV_VALUE: &str = "1";
pub const JACKIN_DIND_HOSTNAME_ENV_NAME: &str = "JACKIN_DIND_HOSTNAME";
pub const TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME: &str = "TESTCONTAINERS_HOST_OVERRIDE";
pub const JACKIN_CONTAINER_NAME_ENV_NAME: &str = "JACKIN_CONTAINER_NAME";
pub const JACKIN_INSTANCE_ID_ENV_NAME: &str = "JACKIN_INSTANCE_ID";
pub const JACKIN_AGENT_ENV_NAME: &str = "JACKIN_AGENT";
/// Unique human-readable codename assigned to a Capsule tab at creation.
///
/// E.g. `"badger"`, `"falcon"`; injected into every process spawned in that
/// tab. Stable across agent restarts and context resets — it is a tab property,
/// not a process property — and never reused within a container lifetime.
pub const JACKIN_AGENT_CODENAME_ENV_NAME: &str = "JACKIN_AGENT_CODENAME";
pub const JACKIN_ROLE_ENV_NAME: &str = "JACKIN_ROLE";
pub const JACKIN_WORKDIR_ENV_NAME: &str = "JACKIN_WORKDIR";
pub const JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME: &str = "JACKIN_GIT_COAUTHOR_TRAILER";
pub const JACKIN_GIT_DCO_ENV_NAME: &str = "JACKIN_GIT_DCO";
pub const ZAI_API_KEY_ENV_NAME: &str = "ZAI_API_KEY";
/// `MiniMax` Token Plan API key. Gates the `MiniMax` provider picker and the
/// `AuthKind::Minimax` env-only auth kind.
pub const MINIMAX_API_KEY_ENV_NAME: &str = "MINIMAX_API_KEY";
/// Kimi API key. Covers both the Kimi Code CLI runtime agent (`api_key` mode)
/// and routing Claude Code / `OpenCode` to Kimi's endpoint. A single key from
/// the Kimi Code Console covers both uses.
pub const KIMI_CODE_API_KEY_ENV_NAME: &str = "KIMI_CODE_API_KEY";
pub const GH_TOKEN_ENV_NAME: &str = "GH_TOKEN";
pub const GITHUB_TOKEN_ENV_NAME: &str = "GITHUB_TOKEN";
pub const GH_HOST_ENV_NAME: &str = "GH_HOST";
pub const GH_ENTERPRISE_TOKEN_ENV_NAME: &str = "GH_ENTERPRISE_TOKEN";

/// All runtime-reserved env var names with their fixed values (or `None` for
/// runtime-generated values).
pub const RESERVED_RUNTIME_ENV_VARS: &[(&str, Option<&str>)] = &[
    (JACKIN_ENV_NAME, Some(JACKIN_ENV_VALUE)),
    (JACKIN_DIND_HOSTNAME_ENV_NAME, None),
    (JACKIN_CONTAINER_NAME_ENV_NAME, None),
    (JACKIN_INSTANCE_ID_ENV_NAME, None),
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
];

/// Returns `true` if `name` is a runtime-reserved env var name.
pub fn is_reserved(name: &str) -> bool {
    RESERVED_RUNTIME_ENV_VARS
        .iter()
        .any(|(reserved, _)| *reserved == name)
}

/// Extract `${env.VAR_NAME}` interpolation placeholder names from a string.
pub fn extract_interpolation_refs(s: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find('}') {
            let ref_expr = &after_open[..end];
            if let Some(var_name) = ref_expr.strip_prefix("env.") {
                refs.push(var_name);
            }
            rest = &after_open[end + 1..];
        } else {
            break;
        }
    }
    refs
}

/// Topologically sort env var declarations by `env.` dependencies.
///
/// Returns names ordered so every dependency precedes its dependents.
///
/// # Errors
/// Returns an error if a dependency cycle is detected.
pub fn topological_env_order(
    declarations: &std::collections::BTreeMap<String, crate::manifest::EnvVarDecl>,
) -> anyhow::Result<Vec<String>> {
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
        anyhow::bail!("env var dependency cycle detected");
    }

    Ok(result)
}
