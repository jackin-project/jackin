// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, FileMode, ModelName, Provider, lookup_tuple,
    tuple_profile_asset,
};
use jackin_openshell::ProviderProfile;
use jackin_session::{HeadState, MultiLayout, PlanError, plan_multi};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

const HOME_CONFIG: &str = "/sandbox/.jackin/homes/codex/.codex/config.toml";

fn cases() -> Vec<(
    Provider,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    vec![
        (
            Provider::Kimi,
            "kimi",
            "Kimi",
            "https://api.kimi.ai/coding/v1",
            "KIMI_API_KEY",
            "kimi-for-coding",
        ),
        (
            Provider::Zai,
            "zai",
            "Z.AI",
            "https://api.z.ai/api/v1",
            "ZAI_API_KEY",
            "glm-5.3",
        ),
        (
            Provider::Minimax,
            "minimax",
            "MiniMax",
            "https://api.minimax.io/v1",
            "MINIMAX_CODE_API_KEY",
            "minimax-m3",
        ),
        (
            Provider::Mimo,
            "mimo",
            "MiMo",
            "https://token-plan-sgp.xiaomimimo.com/v1",
            "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            "mimo-v2.6-pro",
        ),
    ]
}

fn mirror_instances(
    codex_provider: Provider,
    model: &str,
) -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Kimi,
            AccountName::parse("work")?,
            ModelName::parse("kimi-for-coding")?,
        ),
        AgentInstance::new(
            Agent::Codex,
            codex_provider,
            AccountName::parse("work")?,
            ModelName::parse(model)?,
        ),
    ])?)
}

fn home_config(
    codex_provider: Provider,
    model: &str,
) -> Result<(MultiLayout, String), Box<dyn std::error::Error>> {
    let layout = plan_multi(
        &mirror_instances(codex_provider, model)?,
        Agent::ClaudeCode,
        Some("do it"),
        HeadState::Born,
    )?;
    let file = layout
        .files
        .iter()
        .find(|file| file.path.as_str() == HOME_CONFIG)
        .ok_or("no codex home config")?;
    let text = file.text.clone();
    Ok((layout, text))
}

#[test]
fn codex_non_lead_gets_its_exact_home_config_and_the_lead_claude_gets_none()
-> Result<(), Box<dyn std::error::Error>> {
    for (provider, id, name, base_url, env_key, model) in cases() {
        let (layout, text) = home_config(provider, model)?;
        let file = layout
            .files
            .iter()
            .find(|file| file.path.as_str() == HOME_CONFIG)
            .ok_or("no codex home config")?;
        assert_eq!(file.mode, FileMode(0o600), "{id}");
        assert!(file.text.ends_with('\n'), "{id}");
        let expected: toml::Table = toml::from_str(&format!(
            "model = \"{model}\"\nmodel_provider = \"{id}\"\n\n[model_providers.{id}]\nname = \"{name}\"\nbase_url = \"{base_url}\"\nenv_key = \"{env_key}\"\nwire_api = \"responses\"\n\n[projects.\"/sandbox/work/codex/project\"]\ntrust_level = \"trusted\"\n"
        ))?;
        assert_eq!(toml::from_str::<toml::Table>(&text)?, expected, "{id}");
        assert_eq!(
            layout
                .files
                .iter()
                .filter(|file| file.path.as_str().starts_with("/sandbox/.jackin/homes/"))
                .count(),
            1,
            "{id}"
        );
        assert!(
            !layout
                .files
                .iter()
                .any(|file| file.path.as_str().contains("homes/claude")),
            "{id}"
        );
        let lead_worktree = &layout.worktrees[0];
        assert_eq!(lead_worktree.agent, Agent::ClaudeCode, "{id}");
        assert_eq!(lead_worktree.path.as_str(), "/sandbox/project", "{id}");
        let codex_worktree = &layout.worktrees[1];
        assert_eq!(
            codex_worktree.path.as_str(),
            "/sandbox/work/codex/project",
            "{id}"
        );
        assert_eq!(
            codex_worktree.branch.as_deref(),
            Some("agent/codex"),
            "{id}"
        );
    }
    Ok(())
}

#[test]
fn codex_home_config_takes_the_explicit_model_and_refuses_an_unknown_one()
-> Result<(), Box<dyn std::error::Error>> {
    for (provider, id, _, _, _, model) in cases() {
        let (_, text) = home_config(provider, model)?;
        let parsed: toml::Table = toml::from_str(&text)?;
        assert_eq!(parsed["model"].as_str(), Some(model), "{id}");

        let err = plan_multi(
            &mirror_instances(provider, "no-such-model")?,
            Agent::ClaudeCode,
            Some("do it"),
            HeadState::Born,
        )
        .err()
        .ok_or("expected an error")?;
        assert!(
            matches!(&err, PlanError::TupleNotSupported { reason, .. } if reason.contains("unknown model no-such-model")),
            "{id}: {err}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn t01_lead_config_stays_trust_only_with_no_model_provider() -> TestResult {
    guard(async {
        let bare = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let plan = bare.plan(bare.request(1)?.model(Some("gpt-5.1-codex".to_owned())))?;
        let file = &plan.files().codex_config;
        assert_eq!(
            file.path,
            jackin_core::SandboxPath::parse("/sandbox/.codex/config.toml")?
        );
        assert_eq!(file.mode, FileMode(0o600));
        let expected: toml::Table =
            toml::from_str("[projects.\"/sandbox/project\"]\ntrust_level = \"trusted\"\n")?;
        assert_eq!(toml::from_str::<toml::Table>(&file.text)?, expected);

        let modelled = Spec::new(RoleFixture::new("sentinel")?.model("gpt-5.1-codex"))
            .build()
            .await?;
        let plan = modelled.plan(modelled.request(2)?)?;
        let file = &plan.files().codex_config;
        let expected: toml::Table = toml::from_str(
            "model = \"gpt-5.1-codex\"\n\n[projects.\"/sandbox/project\"]\ntrust_level = \"trusted\"\n",
        )?;
        assert_eq!(toml::from_str::<toml::Table>(&file.text)?, expected);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn plan_model_selection_allows_explicit_refuses_outside_and_defaults_without_one()
-> TestResult {
    guard(async {
        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let plan = setup.plan(
            setup
                .request(1)?
                .model(Some("gpt-5.1-codex".to_owned())),
        )?;
        assert_eq!(plan.instances().as_slice().first().ok_or("missing lead")?.model.as_str(), "gpt-5.1-codex");

        let err = setup
            .plan(setup.request(1)?.model(Some("no-such-model".to_owned())))
            .err()
            .ok_or("expected an error")?;
        assert!(
            matches!(&err, PlanError::TupleNotSupported { reason, .. } if reason.contains("unknown model no-such-model")),
            "{err}"
        );

        let plan = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.instances().as_slice().first().ok_or("missing lead")?.model.as_str(), "gpt-5.1-codex");
        Ok(())
    })
    .await
}

#[test]
fn codex_base_url_host_matches_the_tuple_profile_endpoint_host()
-> Result<(), Box<dyn std::error::Error>> {
    for (provider, id, _, base_url, _, _) in cases() {
        let tuple = lookup_tuple(Agent::Codex, provider)?;
        assert_eq!(tuple.codex_base_url(), Some(base_url), "{id}");
        let host = base_url
            .strip_prefix("https://")
            .unwrap_or(base_url)
            .split('/')
            .next()
            .unwrap_or_default();
        let profile = ProviderProfile::parse(tuple_profile_asset(tuple))?;
        assert_eq!(profile.endpoints.len(), 1, "{id}");
        assert_eq!(profile.endpoints[0].host.as_str(), host, "{id}");
    }
    Ok(())
}
