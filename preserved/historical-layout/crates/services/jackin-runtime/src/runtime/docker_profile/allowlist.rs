// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host allowlists for agents and GitHub.

use super::EffectiveGrants;

/// Default agent API endpoints added to `JACKIN_ALLOWED_HOSTS` when
/// `network = "allowlist"` is active. These are the minimum set required
/// for each agent to reach its model API.
pub fn default_allowed_hosts_for_agent(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &["api.anthropic.com"],
        "codex" => &["api.openai.com"],
        "amp" => &["ampcode.com", "sourcegraph.com"],
        "kimi" => &["api.kimi.com", "kimi.moonshot.cn"],
        "opencode" => &["api.z.ai", "api.anthropic.com", "api.openai.com"],
        "grok" => &["api.x.ai"],
        // Google-fronted CLIs reach the Gemini API endpoint.
        "antigravity" | "gemini" => &["generativelanguage.googleapis.com"],
        "cursor" => &["api2.cursor.sh"],
        // Muse API base is provider-configured; no verified default host.
        "muse" => &[],
        // Multi-provider routers reach whichever provider the routed account
        // selects; OpenRouter is the documented default route.
        "omp" | "hermes" => &["openrouter.ai"],
        _ => &[],
    }
}

/// Fixed GitHub egress endpoints added to the allowlist when a GitHub token is
/// forwarded, plus the operator's enterprise `GH_HOST` when set. Sibling policy
/// to [`default_allowed_hosts_for_agent`] — the GitHub half of the egress set.
pub fn github_allowlist_hosts(gh_host: Option<&str>) -> Vec<String> {
    let mut hosts = vec!["github.com".to_owned(), "api.github.com".to_owned()];
    if let Some(host) = gh_host {
        hosts.push(host.to_owned());
    }
    hosts
}

/// WP1: assemble the full egress allowlist injected as `JACKIN_ALLOWED_HOSTS`.
///
/// Union of: operator/role-configured `grants.allowed_hosts`, the agent's
/// default API endpoint(s), any forwarded GitHub host(s), and the OTLP
/// telemetry endpoint host — deduplicated, order-preserving. The OTLP host is
/// jackin❯-owned infrastructure egress (Decision 9): it is always present when
/// telemetry is active and is not operator-removable, so the capsule keeps
/// exporting under `hardened`/`locked`.
///
/// The result is fail-closed by construction: an empty union under
/// `network = allowlist` yields a DROP-only policy in `firewall-apply` (no
/// egress), never open egress.
pub fn allowlist_hosts(
    agent: &str,
    grants: &EffectiveGrants,
    github_hosts: &[String],
    otlp_host: Option<&str>,
) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    let mut push = |h: &str| {
        let h = h.trim();
        if !h.is_empty() && !hosts.iter().any(|existing| existing == h) {
            hosts.push(h.to_owned());
        }
    };
    for h in &grants.allowed_hosts {
        push(h);
    }
    for h in default_allowed_hosts_for_agent(agent) {
        push(h);
    }
    for h in github_hosts {
        push(h);
    }
    if let Some(h) = otlp_host {
        push(h);
    }
    hosts
}
