// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, Provider, lookup_tuple, tuple_profile_asset};
use jackin_openshell::ProviderProfile;
use jackin_session::render_claude_config;
use serde_json::{Value, json};

use super::support::TestResult;

fn cases() -> Vec<(Provider, &'static str, &'static str)> {
    vec![
        (
            Provider::Kimi,
            "https://api.kimi.ai/coding/",
            "ANTHROPIC_API_KEY",
        ),
        (
            Provider::Zai,
            "https://api.z.ai/api/anthropic",
            "ANTHROPIC_AUTH_TOKEN",
        ),
        (
            Provider::Minimax,
            "https://api.minimax.io/anthropic",
            "ANTHROPIC_AUTH_TOKEN",
        ),
        (
            Provider::Mimo,
            "https://token-plan-sgp.xiaomimimo.com/anthropic",
            "ANTHROPIC_AUTH_TOKEN",
        ),
    ]
}

#[test]
fn claude_config_pins_only_the_base_url_and_no_auth_env() -> TestResult {
    for (provider, base_url, auth_env) in cases() {
        let tuple = lookup_tuple(Agent::ClaudeCode, provider)?;
        let rendered = render_claude_config(tuple)?;
        let value: Value = serde_json::from_str(&rendered)?;
        assert_eq!(
            value,
            json!({"env": {"ANTHROPIC_BASE_URL": base_url}}),
            "{}",
            tuple.id
        );
        let root = value.as_object().ok_or("root is not an object")?;
        assert_eq!(root.len(), 1, "{}", tuple.id);
        let env = value["env"].as_object().ok_or("env is not an object")?;
        assert_eq!(env.len(), 1, "{}", tuple.id);
        assert_eq!(value["env"]["ANTHROPIC_BASE_URL"], base_url, "{}", tuple.id);
        assert!(env.get(auth_env).is_none(), "{}", tuple.id);
    }
    Ok(())
}

#[test]
fn claude_base_url_host_matches_the_tuple_profile_endpoint_host() -> TestResult {
    for (provider, base_url, _) in cases() {
        let tuple = lookup_tuple(Agent::ClaudeCode, provider)?;
        let claude_host = base_url
            .strip_prefix("https://")
            .unwrap_or(base_url)
            .split('/')
            .next()
            .unwrap_or_default();
        let profile = ProviderProfile::parse(tuple_profile_asset(tuple))?;
        assert_eq!(profile.endpoints.len(), 1, "{}", tuple.id);
        assert_eq!(
            profile.endpoints[0].host.as_str(),
            claude_host,
            "{}",
            tuple.id
        );
    }
    Ok(())
}
