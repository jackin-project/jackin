// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, AuthClass, CatalogError, FileMode, Provider,
    SandboxPath, TupleState, lookup_tuple, tuple_profile_asset,
};
use jackin_openshell::ProviderProfile;
use jackin_session::{
    HeadState, PlanError, claude_config_file, plan_multi, plan_static_files, render_claude_config,
};
use jackin_testkit::RoleFixture;
use serde_json::{Value, json};

use super::support::{Spec, TestResult, guard};

fn cases() -> Vec<(Provider, &'static str, &'static str)> {
    vec![
        (
            Provider::Kimi,
            "https://api.kimi.ai/coding/",
            "kimi-for-coding",
        ),
        (Provider::Zai, "https://api.z.ai/api/anthropic", "glm-5.3"),
        (
            Provider::Minimax,
            "https://api.minimax.io/anthropic",
            "MiniMax-M3",
        ),
        (
            Provider::Mimo,
            "https://token-plan-sgp.xiaomimimo.com/anthropic",
            "mimo-v2.6-pro",
        ),
    ]
}

fn role() -> Result<RoleFixture, Box<dyn std::error::Error>> {
    Ok(RoleFixture::new("sentinel")?.manifest_text(
        "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n".to_owned(),
    ))
}

fn assert_no_auth(value: &Value, rendered: &str, id: &str) {
    for forbidden in [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "placeholder",
    ] {
        assert!(!rendered.contains(forbidden), "{id}: {forbidden}");
        assert!(value.get(forbidden).is_none(), "{id}: {forbidden}");
    }
}

fn assert_unknown_model(tuple: &'static jackin_core::AgentTuple, rejected: &str) {
    let result = render_claude_config(tuple, Some(rejected));
    assert!(
        matches!(
            &result,
            Err(CatalogError::UnknownModel { agent, provider, model })
                if *agent == tuple.agent && *provider == tuple.provider && model == rejected
        ),
        "{}: {result:?}",
        tuple.id
    );
}

#[test]
fn claude_config_selects_the_exact_tuple_model_without_auth() -> TestResult {
    for (provider, base_url, model) in cases() {
        let tuple = lookup_tuple(Agent::ClaudeCode, provider)?;
        let rendered = render_claude_config(tuple, Some(model))?;
        let value: Value = serde_json::from_str(&rendered)?;
        assert_eq!(
            value,
            json!({"model": model, "env": {"ANTHROPIC_BASE_URL": base_url}}),
            "{}",
            tuple.id
        );
        assert_no_auth(&value, &rendered, tuple.id);
        if provider != Provider::Kimi {
            assert!(!rendered.contains("kimi-for-coding"), "{}", tuple.id);
        }
        let defaulted = render_claude_config(tuple, None)?;
        assert_eq!(
            serde_json::from_str::<Value>(&defaulted)?,
            value,
            "{}",
            tuple.id
        );

        let file = claude_config_file(tuple, Some(model))?;
        assert_eq!(
            file.path,
            SandboxPath::from_static("/sandbox/.claude/settings.json"),
            "{}",
            tuple.id
        );
        assert_eq!(file.mode, FileMode(0o600), "{}", tuple.id);
        assert_eq!(file.text, rendered, "{}", tuple.id);
    }
    Ok(())
}

#[tokio::test]
async fn claude_config_rejects_unknown_and_cross_tuple_models() -> TestResult {
    for provider in [
        Provider::Claude,
        Provider::Kimi,
        Provider::Zai,
        Provider::Minimax,
        Provider::Mimo,
    ] {
        let tuple = lookup_tuple(Agent::ClaudeCode, provider)?;
        assert_unknown_model(tuple, "no-such-model");
    }
    assert_unknown_model(
        lookup_tuple(Agent::ClaudeCode, Provider::Zai)?,
        "kimi-for-coding",
    );
    assert_unknown_model(
        lookup_tuple(Agent::ClaudeCode, Provider::Minimax)?,
        "minimax-m3",
    );

    guard(async {
        let setup = Spec::new(role()?).build().await?;
        let error = setup
            .plan(
                setup
                    .request(1)?
                    .tuple(Agent::ClaudeCode, Provider::Zai)
                    .model(Some("no-such-model".to_owned())),
            )
            .err()
            .ok_or("expected plan_launch unknown model refusal")?;
        assert!(
            matches!(
                &error,
                PlanError::TupleNotSupported { agent, provider, reason }
                    if *agent == Agent::ClaudeCode
                        && *provider == Provider::Zai
                        && reason == "unknown model no-such-model"
            ),
            "{error}"
        );
        Ok(())
    })
    .await
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
    for (provider, base_url, model) in cases() {
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

        let rendered = render_claude_config(tuple, Some(model))?;
        let value: Value = serde_json::from_str(&rendered)?;
        assert_eq!(
            value,
            json!({"model": model, "env": {"ANTHROPIC_BASE_URL": base_url}}),
            "{}",
            tuple.id
        );
        assert_no_auth(&value, &rendered, tuple.id);
    }
    Ok(())
}

#[tokio::test]
async fn t14_plan_launch_propagates_the_lead_model_to_claude_settings() -> TestResult {
    guard(async {
        let setup = Spec::new(role()?).build().await?;
        let plan = setup.plan(
            setup
                .request(1)?
                .tuple(Agent::ClaudeCode, Provider::Zai)
                .model(Some("glm-5.3".to_owned())),
        )?;
        let file = plan
            .files()
            .static_files
            .iter()
            .find(|file| file.path == SandboxPath::from_static("/sandbox/.claude/settings.json"))
            .ok_or("missing Claude settings")?;
        assert_eq!(
            file.path,
            SandboxPath::from_static("/sandbox/.claude/settings.json")
        );
        assert_eq!(file.mode, FileMode(0o600));
        let value: Value = serde_json::from_str(&file.text)?;
        assert_eq!(
            value,
            json!({"model":"glm-5.3","env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic"}})
        );
        assert_no_auth(&value, &file.text, "T14");
        Ok(())
    })
    .await
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
    let rendered = render_claude_config(tuple, None)?;
    let value: Value = serde_json::from_str(&rendered)?;
    assert_eq!(value, json!({"model":"haiku"}), "{}", tuple.id);
    assert!(value.get("env").is_none(), "{}", tuple.id);
    assert_no_auth(&value, &rendered, tuple.id);
    let file = claude_config_file(tuple, Some("haiku"))?;
    assert_eq!(
        file.path,
        SandboxPath::from_static("/sandbox/.claude/settings.json"),
        "{}",
        tuple.id
    );
    assert_eq!(file.mode, FileMode(0o600), "{}", tuple.id);
    assert_eq!(file.text, rendered, "{}", tuple.id);
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
        assert_eq!(settings.text, "{\"model\":\"haiku\"}");
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
