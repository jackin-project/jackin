// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, AuthClass, FileMode, Provider, SandboxPath,
    TupleState, lookup_tuple, tuple_profile_asset,
};
use jackin_openshell::ProviderProfile;
use jackin_session::{HeadState, PlanError, plan_multi, plan_static_files, render_claude_config};
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

#[test]
fn t02_native_placeholder_is_exact_in_lead_and_secondary_homes() -> TestResult {
    let tuple = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    assert_eq!(tuple.auth_class, AuthClass::BlockedVendor);
    assert_eq!(tuple.state, TupleState::BlockedVendor);
    let instance = AgentInstance::new(
        tuple.agent,
        tuple.provider,
        AccountName::parse("claude")?,
        tuple.default_model.to_owned(),
    );
    for (home, expected) in [
        ("/sandbox", "/sandbox/.claude/.credentials.json"),
        (
            "/sandbox/.jackin/homes/claude",
            "/sandbox/.jackin/homes/claude/.claude/.credentials.json",
        ),
    ] {
        let files = plan_static_files(
            &instance,
            &SandboxPath::parse(home)?,
            &SandboxPath::from_static("/sandbox/project"),
            None,
        )?;
        assert_eq!(files.len(), 2);
        let settings = files
            .iter()
            .find(|file| file.path.as_str().ends_with("settings.json"))
            .ok_or("missing Claude settings")?;
        assert_eq!(settings.mode, FileMode(0o600));
        assert_eq!(settings.text, "{}");
        let credentials = files
            .iter()
            .find(|file| file.path.as_str() == expected)
            .ok_or("missing native Claude credentials")?;
        assert_eq!(credentials.mode, FileMode(0o600));
        let document: Value = serde_json::from_str(&credentials.text)?;
        assert_eq!(
            document,
            json!({"claudeAiOauth": {
                "accessToken": "openshell:resolve:env:CLAUDE_CODE_OAUTH_TOKEN",
                "refreshToken": "",
                "expiresAt": 4_102_444_800_000_u64,
                "scopes": ["user:profile", "user:inference"]
            }})
        );
        assert!(!credentials.text.contains("ANTHROPIC_API_KEY"));
        assert!(!credentials.text.contains("ANTHROPIC_AUTH_TOKEN"));
    }
    Ok(())
}
