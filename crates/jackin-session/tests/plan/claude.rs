// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, Provider, lookup_tuple, tuple_profile_asset,
};
use jackin_openshell::ProviderProfile;
use jackin_session::{HeadState, PlanError, plan_multi, render_claude_config};
use serde_json::{Value, json};

use super::support::TestResult;

fn cases() -> Vec<(Provider, &'static str, &'static str)> {
    vec![
        (
            Provider::Kimi,
            "https://api.kimi.ai/coding/",
            "ANTHROPIC_AUTH_TOKEN",
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
        let rendered = render_claude_config(tuple);
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

#[test]
fn claude_compatible_tuples_agree_on_auth_token_profile_and_settings() -> TestResult {
    for (provider, base_url, _) in cases() {
        let tuple = lookup_tuple(Agent::ClaudeCode, provider)?;
        assert_eq!(
            tuple.credential_env(),
            Some("ANTHROPIC_AUTH_TOKEN"),
            "{}",
            tuple.id
        );
        let profile = ProviderProfile::parse(tuple_profile_asset(tuple))?;
        assert_eq!(profile.yaml.matches("env_vars:").count(), 1, "{}", tuple.id);
        assert!(
            profile.yaml.contains("env_vars: [ANTHROPIC_AUTH_TOKEN]"),
            "{}",
            tuple.id
        );
        assert_eq!(profile.endpoints.len(), 1, "{}", tuple.id);
        let expected_host = base_url
            .strip_prefix("https://")
            .unwrap_or(base_url)
            .split('/')
            .next()
            .unwrap_or_default();
        assert_eq!(
            profile.endpoints[0].host.as_str(),
            expected_host,
            "{}",
            tuple.id
        );
        for forbidden in [
            "ANTHROPIC_API_KEY",
            "api.anthropic.com",
            "platform.claude.com",
        ] {
            assert!(!profile.yaml.contains(forbidden), "{}", tuple.id);
        }

        let rendered = render_claude_config(tuple);
        let value: Value = serde_json::from_str(&rendered)?;
        assert_eq!(
            value,
            json!({"env": {"ANTHROPIC_BASE_URL": base_url}}),
            "{}",
            tuple.id
        );
        assert!(!rendered.contains("ANTHROPIC_AUTH_TOKEN"), "{}", tuple.id);
    }
    Ok(())
}

#[test]
fn t15_accepts_uppercase_model_validation_but_refuses_launch_while_unproven() -> TestResult {
    let tuple = lookup_tuple(Agent::ClaudeCode, Provider::Minimax)?;
    assert_eq!(tuple.check_model("MiniMax-M3")?.id, "T15");
    let instances = AgentInstances::single(AgentInstance::new(
        Agent::ClaudeCode,
        Provider::Minimax,
        AccountName::parse("work")?,
        "MiniMax-M3".to_owned(),
    ));
    let error = plan_multi(&instances, None, HeadState::Born)
        .err()
        .ok_or("expected unavailable tuple refusal")?;
    assert!(
        matches!(
            error,
            PlanError::TupleNotSupported { ref reason, .. }
                if reason == "uppercase MiniMax-M3 live proof pending"
        ),
        "{error}"
    );
    Ok(())
}

#[test]
fn t02_subscription_settings_carry_no_base_url_and_no_auth_env() -> TestResult {
    let tuple = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    let value: Value = serde_json::from_str(&render_claude_config(tuple))?;
    assert_eq!(value, json!({}), "{}", tuple.id);
    Ok(())
}

#[test]
fn t02_subscription_credential_names_one_oauth_env() -> TestResult {
    let tuple = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    assert_eq!(tuple.claude_auth_env(), Some("CLAUDE_CODE_OAUTH_TOKEN"));
    assert!(
        tuple_profile_asset(tuple)
            .yaml
            .contains("env_vars: [CLAUDE_CODE_OAUTH_TOKEN]"),
        "{}",
        tuple.id
    );
    Ok(())
}
