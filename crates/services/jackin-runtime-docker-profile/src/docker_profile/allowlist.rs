// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::EffectiveGrants;

pub fn default_allowed_hosts_for_agent(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &["api.anthropic.com"],
        "codex" => &["api.openai.com"],
        "amp" => &["ampcode.com", "sourcegraph.com"],
        "kimi" => &["api.kimi.com", "kimi.moonshot.cn"],
        "opencode" => &["api.z.ai", "api.anthropic.com", "api.openai.com"],
        "grok" => &["api.x.ai"],
        "antigravity" | "gemini" => &["generativelanguage.googleapis.com"],
        "cursor" => &["api2.cursor.sh"],
        "muse" => &[],
        "omp" | "hermes" => &["openrouter.ai"],
        _ => &[],
    }
}

pub fn github_allowlist_hosts(gh_host: Option<&str>) -> Vec<String> {
    let mut hosts = vec!["github.com".to_owned(), "api.github.com".to_owned()];
    if let Some(host) = gh_host {
        hosts.push(host.to_owned());
    }
    hosts
}

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
