// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, FileMode, ModelName, Provider, SandboxPath,
    lookup_tuple, tuple_profile_asset,
};
use jackin_openshell::ProviderProfile;
use jackin_session::{HeadState, MultiLayout, PlanError, plan_multi, render_codex_config};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

const HOME_CONFIG: &str = "/sandbox/.jackin/homes/codex/.codex/config.toml";

type Case = (
    Provider,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);

fn cases() -> Vec<Case> {
    vec![
        (
            Provider::Kimi,
            "kimi",
            "Kimi",
            "https://api.kimi.ai/coding/v1",
            "KIMI_API_KEY",
            "kimi-for-coding",
            None,
        ),
        (
            Provider::Zai,
            "zai",
            "Z.AI",
            "https://api.z.ai/api/v1",
            "ZAI_API_KEY",
            "glm-5.3",
            None,
        ),
        (
            Provider::Minimax,
            "minimax",
            "MiniMax",
            "https://api.minimax.io/v1",
            "MINIMAX_CODE_API_KEY",
            "MiniMax-M3",
            None,
        ),
        (
            Provider::Mimo,
            "mimo",
            "MiMo",
            "https://token-plan-sgp.xiaomimimo.com/v1",
            "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            "mimo-v2.6-pro",
            Some("disabled"),
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
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::Codex,
            codex_provider,
            AccountName::parse("work")?,
            model.to_owned(),
        ),
    ])?)
}

fn home_config(
    codex_provider: Provider,
    model: &str,
) -> Result<(MultiLayout, Vec<jackin_session::PlannedFile>), Box<dyn std::error::Error>> {
    let instances = mirror_instances(codex_provider, model)?;
    let layout = plan_multi(&instances, Some("do it"), HeadState::Born)?;
    let mut files = Vec::new();
    for (index, (instance, exec)) in instances.iter().zip(&layout.exec).enumerate() {
        let home = if index == 0 {
            SandboxPath::from_static("/sandbox")
        } else {
            exec.home.clone()
        };
        files.extend(jackin_session::plan_static_files(
            instance,
            &home,
            &exec.workdir,
            None,
        )?);
    }
    Ok((layout, files))
}

#[test]
fn codex_non_lead_gets_its_exact_home_config_and_the_lead_claude_gets_none()
-> Result<(), Box<dyn std::error::Error>> {
    for (provider, id, name, base_url, env_key, model, web_search) in cases() {
        let (layout, files) = home_config(provider, model)?;
        let file = files
            .iter()
            .find(|file| file.path.as_str() == HOME_CONFIG)
            .ok_or("no codex home config")?;
        assert_eq!(file.mode, FileMode(0o600), "{id}");
        assert!(file.text.ends_with('\n'), "{id}");
        let web_search = web_search
            .map(|value| format!("web_search = \"{value}\"\n"))
            .unwrap_or_default();
        let expected: toml::Table = toml::from_str(&format!(
            "model = \"{model}\"\nmodel_provider = \"{id}\"\n{web_search}\n[model_providers.{id}]\nname = \"{name}\"\nbase_url = \"{base_url}\"\nenv_key = \"{env_key}\"\nwire_api = \"responses\"\n\n[projects.\"/sandbox/work/codex/project\"]\ntrust_level = \"trusted\"\n"
        ))?;
        assert_eq!(toml::from_str::<toml::Table>(&file.text)?, expected, "{id}");
        assert_eq!(
            files
                .iter()
                .filter(|file| file.path.as_str().starts_with("/sandbox/.jackin/homes/"))
                .count(),
            1,
            "{id}"
        );
        assert!(
            !files
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
    for (provider, id, _, _, _, model, _) in cases() {
        let (_, files) = home_config(provider, model)?;
        let file = files
            .iter()
            .find(|file| file.path.as_str() == HOME_CONFIG)
            .ok_or("no codex home config")?;
        let parsed: toml::Table = toml::from_str(&file.text)?;
        assert_eq!(parsed["model"].as_str(), Some(model), "{id}");

        let err = plan_multi(
            &mirror_instances(provider, "no-such-model")?,
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

#[test]
fn t17_plans_and_renders_the_exact_kimi_provider_and_model() -> TestResult {
    let (layout, text) = home_config(Provider::Kimi, "kimi-for-coding")?;
    let parsed: toml::Table = toml::from_str(&text)?;
    assert_eq!(parsed["model"].as_str(), Some("kimi-for-coding"));
    assert_eq!(parsed["model_provider"].as_str(), Some("kimi"));
    assert_eq!(layout.exec[1].provider, "kimi");
    assert_eq!(layout.exec[1].model, "kimi-for-coding");
    Ok(())
}

#[test]
fn t19_plans_and_renders_exact_uppercase_minimax_model() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Minimax)?;
    assert_eq!(tuple.check_model("MiniMax-M3")?.id, "T19");
    let (layout, text) = home_config(Provider::Minimax, "MiniMax-M3")?;
    let parsed: toml::Table = toml::from_str(&text)?;
    assert_eq!(parsed["model"].as_str(), Some("MiniMax-M3"));
    assert_eq!(parsed["model_provider"].as_str(), Some("minimax"));
    assert_eq!(layout.exec[1].provider, "minimax");
    assert_eq!(layout.exec[1].model, "MiniMax-M3");
    Ok(())
}

#[test]
fn t01_pure_rendering_stays_trust_only_with_no_model_provider() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Codex)?;
    let workdir = SandboxPath::parse("/sandbox/project")?;
    let bare = render_codex_config(tuple, None, Some("gpt-6.1-sol"), &workdir)?;
    let expected: toml::Table =
        toml::from_str("[projects.\"/sandbox/project\"]\ntrust_level = \"trusted\"\n")?;
    assert_eq!(toml::from_str::<toml::Table>(&bare)?, expected);

    let model = ModelName::parse("gpt-6.1-sol")?;
    let modelled = render_codex_config(tuple, Some(&model), Some("gpt-6.1-sol"), &workdir)?;
    let expected: toml::Table = toml::from_str(
        "model = \"gpt-6.1-sol\"\n\n[projects.\"/sandbox/project\"]\ntrust_level = \"trusted\"\n",
    )?;
    assert_eq!(toml::from_str::<toml::Table>(&modelled)?, expected);
    Ok(())
}

#[test]
fn t20_pure_rendering_disables_web_search() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Mimo)?;
    let workdir = SandboxPath::parse("/sandbox/work/codex/project")?;
    let text = render_codex_config(tuple, None, Some("mimo-v2.6-pro"), &workdir)?;
    let expected: toml::Table = toml::from_str(
        "model = \"mimo-v2.6-pro\"\nmodel_provider = \"mimo\"\nweb_search = \"disabled\"\n\n[model_providers.mimo]\nname = \"MiMo\"\nbase_url = \"https://token-plan-sgp.xiaomimimo.com/v1\"\nenv_key = \"XIAOMI_TOKEN_PLAN_SGP_API_KEY\"\nwire_api = \"responses\"\n\n[projects.\"/sandbox/work/codex/project\"]\ntrust_level = \"trusted\"\n",
    )?;
    assert_eq!(toml::from_str::<toml::Table>(&text)?, expected);
    Ok(())
}

#[tokio::test]
async fn plan_model_selection_allows_explicit_refuses_outside_and_defaults_without_one()
-> TestResult {
    guard(async {
        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let plan = setup.plan(
            setup
                .request(1)?
                .model(Some("glm-5.3".to_owned())),
        )?;
        assert_eq!(plan.instances().lead().model, "glm-5.3");

        let removed = setup
            .plan(setup.request(1)?.model(Some("gpt-5.1-codex".to_owned())))
            .err()
            .ok_or("expected removed model refusal")?;
        assert!(
            matches!(&removed, PlanError::TupleNotSupported { reason, .. } if reason == "unknown model gpt-5.1-codex"),
            "{removed}"
        );

        let err = setup
            .plan(setup.request(1)?.model(Some("no-such-model".to_owned())))
            .err()
            .ok_or("expected an error")?;
        assert!(
            matches!(&err, PlanError::TupleNotSupported { reason, .. } if reason.contains("unknown model no-such-model")),
            "{err}"
        );

        let plan = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.instances().lead().model, "glm-5.3");
        Ok(())
    })
    .await
}

#[test]
fn codex_base_url_host_matches_the_tuple_profile_endpoint_host()
-> Result<(), Box<dyn std::error::Error>> {
    for (provider, id, _, base_url, _, _, _) in cases() {
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
