// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Env policy model: reserved runtime env vars and interpolation parsing.
//!
//! Single source of truth for:
//! * Names and default values of runtime-reserved environment variables.
//! * `${env.VAR_NAME}` interpolation reference parsing.

/// Env var injected by jackin into every role container.
pub const JACKIN_ENV_NAME: &str = "JACKIN";
/// Value for [`JACKIN_ENV_NAME`].
pub const JACKIN_ENV_VALUE: &str = "1";
/// Hostname of the `DinD` sidecar when Docker-in-Docker is enabled.
pub const JACKIN_DIND_HOSTNAME_ENV_NAME: &str = "JACKIN_DIND_HOSTNAME";
/// Testcontainers host override pointing at the DinD/network endpoint.
pub const TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME: &str = "TESTCONTAINERS_HOST_OVERRIDE";
/// Docker name of the role container.
pub const JACKIN_CONTAINER_NAME_ENV_NAME: &str = "JACKIN_CONTAINER_NAME";
/// Stable instance id for the running container.
pub const JACKIN_INSTANCE_ID_ENV_NAME: &str = "JACKIN_INSTANCE_ID";
/// Agent slug for the session (`claude`, `codex`, …).
pub const JACKIN_AGENT_ENV_NAME: &str = "JACKIN_AGENT";
/// Unique human-readable codename assigned to a Capsule tab at creation.
///
/// E.g. `"badger"`, `"falcon"`; injected into every process spawned in that
/// tab. Stable across agent restarts and context resets — it is a tab property,
/// not a process property — and never reused within a container lifetime.
pub const JACKIN_AGENT_CODENAME_ENV_NAME: &str = "JACKIN_AGENT_CODENAME";
/// Role key / selector for the running container.
pub const JACKIN_ROLE_ENV_NAME: &str = "JACKIN_ROLE";
/// Container working directory set at launch.
pub const JACKIN_WORKDIR_ENV_NAME: &str = "JACKIN_WORKDIR";
/// Git co-author trailer text jackin❯ injects when configured.
pub const JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME: &str = "JACKIN_GIT_COAUTHOR_TRAILER";
/// Whether DCO sign-off is enabled for this container (`1` / absent).
pub const JACKIN_GIT_DCO_ENV_NAME: &str = "JACKIN_GIT_DCO";
/// Per-container opt-out for host browser-open affordances. `deny`, `off`,
/// and `no` suppress explicit jackin❯ host-open URL actions while leaving
/// normal terminal OSC 8 passthrough under `JACKIN_OSC_HYPERLINK`.
pub const JACKIN_OPEN_LINKS_ENV_NAME: &str = "JACKIN_OPEN_LINKS";
/// Z.AI API key env name.
pub const ZAI_API_KEY_ENV_NAME: &str = "ZAI_API_KEY";
/// Anthropic API key env name (Claude `api_key` mode).
pub const ANTHROPIC_API_KEY_ENV_NAME: &str = "ANTHROPIC_API_KEY";
/// `OpenAI` API key env name (Codex `api_key` mode).
pub const OPENAI_API_KEY_ENV_NAME: &str = "OPENAI_API_KEY";
/// Amp API key env name.
pub const AMP_API_KEY_ENV_NAME: &str = "AMP_API_KEY";
/// Claude Code OAuth token env name.
pub const CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME: &str = "CLAUDE_CODE_OAUTH_TOKEN";
/// `MiniMax` Token Plan API key. Gates the `MiniMax` provider picker and the
/// `AuthKind::Minimax` env-only auth kind.
pub const MINIMAX_API_KEY_ENV_NAME: &str = "MINIMAX_API_KEY";
/// Kimi API key. Covers both the Kimi Code CLI runtime agent (`api_key` mode)
/// and routing Claude Code / `OpenCode` to Kimi's endpoint. A single key from
/// the Kimi Code Console covers both uses.
pub const KIMI_CODE_API_KEY_ENV_NAME: &str = "KIMI_CODE_API_KEY";
/// Alternate Kimi API key env name accepted by the Kimi Code CLI.
pub const KIMI_API_KEY_ENV_NAME: &str = "KIMI_API_KEY";
/// `OpenCode` API key env name.
pub const OPENCODE_API_KEY_ENV_NAME: &str = "OPENCODE_API_KEY";
/// xAI API key env name (Grok Build).
pub const XAI_API_KEY_ENV_NAME: &str = "XAI_API_KEY";
/// GitHub CLI token env name (`gh`).
pub const GH_TOKEN_ENV_NAME: &str = "GH_TOKEN";
/// GitHub Actions / generic token env name.
pub const GITHUB_TOKEN_ENV_NAME: &str = "GITHUB_TOKEN";
/// GitHub Enterprise host for `gh`.
pub const GH_HOST_ENV_NAME: &str = "GH_HOST";
/// GitHub Enterprise token for `gh`.
pub const GH_ENTERPRISE_TOKEN_ENV_NAME: &str = "GH_ENTERPRISE_TOKEN";

/// Network mode injected by jackin into role containers (`allowlist`, `open`, `none`).
pub const JACKIN_NETWORK_MODE_ENV_NAME: &str = "JACKIN_NETWORK_MODE";
/// Comma-separated list of allowlisted hostnames/IPs when `JACKIN_NETWORK_MODE=allowlist`.
pub const JACKIN_ALLOWED_HOSTS_ENV_NAME: &str = "JACKIN_ALLOWED_HOSTS";
/// Informational flag set by the entrypoint after firewall-apply completes.
/// Informational only — must not error on absence (firewall installs post-start).
pub const JACKIN_FIREWALL_INSTALLED_ENV_NAME: &str = "JACKIN_FIREWALL_INSTALLED";
/// Network enforcement quality label: `full`, `partial (sudo grants iptables access)`, etc.
pub const JACKIN_NETWORK_ENFORCEMENT_ENV_NAME: &str = "JACKIN_NETWORK_ENFORCEMENT";
/// Set to `1` when the `sudo` grant is active; absent otherwise.
/// The capsule entrypoint writes `/etc/sudoers.d/agent` only when this env is present.
pub const JACKIN_SUDO_ENV_NAME: &str = "JACKIN_SUDO";

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
    (JACKIN_NETWORK_MODE_ENV_NAME, None),
    (JACKIN_ALLOWED_HOSTS_ENV_NAME, None),
    (JACKIN_FIREWALL_INSTALLED_ENV_NAME, None),
    (JACKIN_NETWORK_ENFORCEMENT_ENV_NAME, None),
    (JACKIN_SUDO_ENV_NAME, None),
];

/// Returns `true` if `name` is a runtime-reserved env var name.
pub fn is_reserved(name: &str) -> bool {
    RESERVED_RUNTIME_ENV_VARS
        .iter()
        .any(|(reserved, _)| *reserved == name)
}

/// Shared boolean-deny convention used by operator-controlled environment
/// switches. The exact accepted values intentionally match the OSC passthrough
/// gates so safety controls read consistently across docs and code.
pub fn env_value_is_deny(value: &str) -> bool {
    matches!(value, "deny" | "off" | "no")
}

/// Return whether a host URL-open action is allowed for the given
/// `JACKIN_OPEN_LINKS` value.
pub fn open_links_allowed(value: Option<&str>) -> bool {
    value.is_none_or(|value| !env_value_is_deny(value))
}

/// Extract `${env.VAR_NAME}` interpolation placeholder names from a string.
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

/// Topologically sort env var declarations by `env.` dependencies.
///
/// Returns names ordered so every dependency precedes its dependents.
///
/// Env var dependency graph contains a cycle.
#[derive(Debug, thiserror::Error)]
#[error("env var dependency cycle detected")]
pub struct EnvCycleError;

/// # Errors
/// Returns [`EnvCycleError`] if a dependency cycle is detected.
#[allow(
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
