// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{Agent, Provider, lookup_tuple, tuple_profile_asset};
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

#[test]
fn role_rule_matching_provider_host_is_rejected() -> TestResult {
    let tuple = lookup_tuple(Agent::Omp, Provider::Zai)?;
    let text = "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  custom:\n    name: custom\n    endpoints:\n      - host: api.z.ai\n        port: 443\n    binaries:\n      - path: /usr/local/bin/omp\n";
    let result = AuthoredPolicy::parse(text)?.close_for_tuple(tuple);
    assert!(result.is_err(), "{result:?}");
    Ok(())
}
