// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{Agent, Provider, TUPLES, lookup_tuple, tuple_profile_asset};
use jackin_openshell::{AuthoredPolicy, ProviderProfile};

type TestResult = Result<(), Box<dyn Error>>;

fn hosts(profile: &ProviderProfile) -> Vec<String> {
    profile
        .endpoints
        .iter()
        .map(|e| e.host.to_string())
        .collect()
}

#[test]
fn t01_profile_is_codex_only_with_refresh() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Codex)?;
    let asset = tuple_profile_asset(tuple);
    let profile = ProviderProfile::parse(asset)?;
    assert_eq!(
        profile
            .binaries
            .iter()
            .map(jackin_core::SandboxPath::as_str)
            .collect::<Vec<_>>(),
        ["/usr/local/bin/codex"]
    );
    assert!(hosts(&profile).iter().all(|h| {
        [
            "auth.openai.com",
            "api.openai.com",
            "chatgpt.com",
            "ab.chatgpt.com",
        ]
        .contains(&h.as_str())
    }));
    assert!(asset.yaml.contains("refresh:"));
    assert!(asset.yaml.matches("- name:").count() >= 1);
    let closed = AuthoredPolicy::parse("version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  existing:\n    name: existing\n    endpoints:\n      - host: api.example.com\n        port: 443\n    binaries:\n      - path: /usr/bin/curl\n")?.close_for_tuple(tuple);
    assert!(closed.is_ok(), "{closed:?}");
    Ok(())
}

#[test]
fn t02_spec_permits_supervised_claude_hosts() {
    assert_eq!(
        Agent::ClaudeCode.spec().provider_hosts,
        &["api.anthropic.com", "platform.claude.com"]
    );
}

#[test]
fn t09_profile_is_omp_zai_only_without_refresh() -> TestResult {
    let tuple = lookup_tuple(Agent::Omp, Provider::Zai)?;
    let asset = tuple_profile_asset(tuple);
    let profile = ProviderProfile::parse(asset)?;
    assert_eq!(
        profile
            .binaries
            .iter()
            .map(jackin_core::SandboxPath::as_str)
            .collect::<Vec<_>>(),
        ["/usr/local/bin/omp"]
    );
    assert_eq!(hosts(&profile), ["api.z.ai"]);
    assert!(!asset.yaml.contains("refresh:"));
    Ok(())
}

struct Row {
    id: &'static str,
    asset: &'static str,
    hosts: &'static [&'static str],
    env: &'static str,
    binary: &'static str,
    refresh: bool,
}

const PINNED_ROWS: &[Row] = &[
    Row {
        id: "T01",
        asset: "codex",
        hosts: &[
            "api.openai.com",
            "auth.openai.com",
            "chatgpt.com",
            "ab.chatgpt.com",
        ],
        env: "CODEX_AUTH_ACCESS_TOKEN",
        binary: "/usr/local/bin/codex",
        refresh: true,
    },
    Row {
        id: "T02",
        asset: "claude",
        hosts: &["api.anthropic.com", "platform.claude.com"],
        env: "CLAUDE_CODE_OAUTH_TOKEN",
        binary: "/usr/local/bin/claude",
        refresh: true,
    },
    Row {
        id: "T03",
        asset: "grok",
        hosts: &["cli-chat-proxy.grok.com", "auth.x.ai"],
        env: "GROK_SESSION_TOKEN",
        binary: "/usr/local/bin/grok",
        refresh: true,
    },
    Row {
        id: "T04",
        asset: "muse",
        hosts: &["api.meta.ai"],
        env: "META_API_KEY",
        binary: "/usr/local/bin/muse",
        refresh: false,
    },
    Row {
        id: "T05",
        asset: "agy",
        hosts: &["antigravity.google"],
        env: "GOOGLE_API_KEY",
        binary: "/usr/local/bin/agy",
        refresh: false,
    },
    Row {
        id: "T06",
        asset: "minimax-code",
        hosts: &["api.minimax.io"],
        env: "MINIMAX_API_KEY",
        binary: "/usr/local/bin/mcode",
        refresh: false,
    },
    Row {
        id: "T07",
        asset: "mimo",
        hosts: &["token-plan-sgp.xiaomimimo.com"],
        env: "MIMO_API_KEY",
        binary: "/usr/local/bin/mimo",
        refresh: false,
    },
    Row {
        id: "T08",
        asset: "kimi",
        hosts: &["api.kimi.com", "auth.kimi.com"],
        env: "KIMI_API_KEY",
        binary: "/usr/local/bin/kimi",
        refresh: true,
    },
    Row {
        id: "T09",
        asset: "omp-zai",
        hosts: &["api.z.ai"],
        env: "ZAI_API_KEY",
        binary: "/usr/local/bin/omp",
        refresh: false,
    },
    Row {
        id: "T10",
        asset: "omp-kimi",
        hosts: &["api.kimi.com"],
        env: "KIMI_API_KEY",
        binary: "/usr/local/bin/omp",
        refresh: false,
    },
    Row {
        id: "T11",
        asset: "omp-minimax",
        hosts: &["api.minimax.io"],
        env: "MINIMAX_CODE_API_KEY",
        binary: "/usr/local/bin/omp",
        refresh: false,
    },
    Row {
        id: "T12",
        asset: "omp-mimo",
        hosts: &["token-plan-sgp.xiaomimimo.com"],
        env: "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
        binary: "/usr/local/bin/omp",
        refresh: false,
    },
    Row {
        id: "T13",
        asset: "t13-claude-kimi",
        hosts: &["api.kimi.ai"],
        env: "ANTHROPIC_AUTH_TOKEN",
        binary: "/usr/local/bin/claude",
        refresh: false,
    },
    Row {
        id: "T14",
        asset: "t14-claude-zai",
        hosts: &["api.z.ai"],
        env: "ANTHROPIC_AUTH_TOKEN",
        binary: "/usr/local/bin/claude",
        refresh: false,
    },
    Row {
        id: "T15",
        asset: "t15-claude-minimax",
        hosts: &["api.minimax.io"],
        env: "ANTHROPIC_AUTH_TOKEN",
        binary: "/usr/local/bin/claude",
        refresh: false,
    },
    Row {
        id: "T16",
        asset: "t16-claude-mimo",
        hosts: &["token-plan-sgp.xiaomimimo.com"],
        env: "ANTHROPIC_AUTH_TOKEN",
        binary: "/usr/local/bin/claude",
        refresh: false,
    },
    Row {
        id: "T17",
        asset: "t17-codex-kimi",
        hosts: &["api.kimi.ai"],
        env: "KIMI_API_KEY",
        binary: "/usr/local/bin/codex",
        refresh: false,
    },
    Row {
        id: "T18",
        asset: "t18-codex-zai",
        hosts: &["api.z.ai"],
        env: "ZAI_API_KEY",
        binary: "/usr/local/bin/codex",
        refresh: false,
    },
    Row {
        id: "T19",
        asset: "t19-codex-minimax",
        hosts: &["api.minimax.io"],
        env: "MINIMAX_CODE_API_KEY",
        binary: "/usr/local/bin/codex",
        refresh: false,
    },
    Row {
        id: "T20",
        asset: "t20-codex-mimo",
        hosts: &["token-plan-sgp.xiaomimimo.com"],
        env: "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
        binary: "/usr/local/bin/codex",
        refresh: false,
    },
    Row {
        id: "T21",
        asset: "t21-omp-anthropic",
        hosts: &["api.anthropic.com"],
        env: "ANTHROPIC_API_KEY",
        binary: "/usr/local/bin/omp",
        refresh: false,
    },
    Row {
        id: "T22",
        asset: "t22-omp-openai",
        hosts: &["api.openai.com"],
        env: "OPENAI_API_KEY",
        binary: "/usr/local/bin/omp",
        refresh: false,
    },
];

#[test]
fn every_tuple_profile_parses_with_exact_asset_hosts_env_binary_and_refresh() -> TestResult {
    assert_eq!(TUPLES.len(), PINNED_ROWS.len());
    for row in PINNED_ROWS {
        let tuple = TUPLES
            .iter()
            .find(|tuple| tuple.id == row.id)
            .unwrap_or_else(|| panic!("missing tuple {}", row.id));
        let asset = tuple_profile_asset(tuple);
        assert_eq!(asset.id, row.asset, "{}", row.id);
        let yaml_id = asset
            .yaml
            .lines()
            .find_map(|line| line.strip_prefix("id:"))
            .unwrap_or_else(|| panic!("missing id line in {}", row.id))
            .trim();
        assert_eq!(yaml_id, row.asset, "{}", row.id);
        let profile = ProviderProfile::parse(asset)?;
        assert_eq!(
            profile
                .binaries
                .iter()
                .map(jackin_core::SandboxPath::as_str)
                .collect::<Vec<_>>(),
            [row.binary],
            "{}",
            row.id
        );
        assert_eq!(hosts(&profile), row.hosts, "{}", row.id);
        assert!(
            asset.yaml.contains(&format!("env_vars: [{}]", row.env)),
            "{}",
            row.id
        );
        assert_eq!(asset.yaml.contains("refresh:"), row.refresh, "{}", row.id);
    }
    Ok(())
}

#[test]
fn role_rule_matching_provider_host_is_rejected() -> TestResult {
    let tuple = lookup_tuple(Agent::Omp, Provider::Zai)?;
    let text = "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  custom:\n    name: custom\n    endpoints:\n      - host: api.z.ai\n        port: 443\n    binaries:\n      - path: /usr/local/bin/omp\n";
    let result = AuthoredPolicy::parse(text)?.close_for_tuple(tuple);
    assert!(result.is_err(), "{result:?}");
    Ok(())
}

#[test]
fn t02_role_rules_matching_provider_hosts_are_rejected() -> TestResult {
    let tuple = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    for host in ["api.anthropic.com", "platform.claude.com"] {
        let text = format!(
            "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  custom:\n    name: custom\n    endpoints:\n      - host: {host}\n        port: 443\n    binaries:\n      - path: /usr/local/bin/claude\n"
        );
        let result = AuthoredPolicy::parse(&text)?.close_for_tuple(tuple);
        assert!(result.is_err(), "{host}: {result:?}");
    }
    Ok(())
}

fn t08_global_policy(host: &str) -> String {
    format!(
        "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  custom:\n    name: custom\n    endpoints:\n      - host: {host}\n        port: 443\n    binaries:\n      - path: /usr/local/bin/kimi\n"
    )
}

#[test]
fn t08_global_role_rules_matching_single_instance_provider_hosts_are_rejected() -> TestResult {
    let tuple = lookup_tuple(Agent::KimiCode, Provider::Kimi)?;
    for host in ["api.kimi.ai", "auth.kimi.ai"] {
        let result = AuthoredPolicy::parse(&t08_global_policy(host))?.close_for_tuple(tuple);
        assert!(result.is_err(), "{host}: {result:?}");
    }
    Ok(())
}

#[test]
fn t08_global_role_rules_matching_multi_instance_provider_hosts_are_rejected() -> TestResult {
    let instances = jackin_core::AgentInstances::new(vec![
        jackin_core::AgentInstance::new(
            Agent::KimiCode,
            Provider::Kimi,
            jackin_core::AccountName::parse("native")?,
            "kimi-for-coding".to_owned(),
        ),
        jackin_core::AgentInstance::new(
            Agent::Codex,
            Provider::Codex,
            jackin_core::AccountName::parse("work")?,
            "gpt-6.1-sol".to_owned(),
        ),
    ])?;
    for host in ["api.kimi.ai", "auth.kimi.ai"] {
        let result =
            AuthoredPolicy::parse(&t08_global_policy(host))?.close_for_instances(&instances);
        assert!(result.is_err(), "{host}: {result:?}");
    }
    Ok(())
}
