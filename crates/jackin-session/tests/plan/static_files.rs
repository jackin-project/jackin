// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, FileMode, Provider, SandboxPath, lookup_tuple,
};
use jackin_session::{PlanError, plan_static_files};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

const HOME: SandboxPath = SandboxPath::from_static("/sandbox");
const WORKDIR: SandboxPath = SandboxPath::from_static("/sandbox/project");
const ROLE: &str = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n\n[docker]\nenabled = false\n";

fn instance(
    agent: Agent,
    provider: Provider,
    model: &str,
) -> Result<AgentInstance, Box<dyn std::error::Error>> {
    Ok(AgentInstance::new(
        agent,
        provider,
        AccountName::parse("zai")?,
        model.to_owned(),
    ))
}

fn file<'a>(
    files: &'a [jackin_session::PlannedFile],
    path: &str,
) -> Result<&'a jackin_session::PlannedFile, Box<dyn std::error::Error>> {
    files
        .iter()
        .find(|file| file.path.as_str() == path)
        .ok_or_else(|| format!("no file at {path}").into())
}

#[test]
fn lead_static_files_pin_t09_t14_and_t18_paths_modes_and_bytes() -> TestResult {
    let t09 = plan_static_files(
        &instance(Agent::Omp, Provider::Zai, "glm-5.3")?,
        &HOME,
        &WORKDIR,
        None,
    )?;
    assert_eq!(t09.len(), 3);
    let config = file(&t09, "/sandbox/.omp/config.toml")?;
    assert_eq!(config.mode, FileMode(0o600));
    assert_eq!(
        config.text,
        "providers:\n  zai:\n    baseUrl: https://api.z.ai/api/anthropic\n    apiKey: placeholder\ncredentialEnv: ZAI_API_KEY\nmodelRoles:\n  default: zai/glm-5.3\n"
    );
    let auth = file(&t09, "/sandbox/.omp/auth.json")?;
    assert_eq!(auth.mode, FileMode(0o600));
    assert_eq!(
        auth.text,
        "{\"zai\": {\"type\": \"api_key\", \"key\": \"placeholder\"}}"
    );
    let models = file(&t09, "/sandbox/.omp/agent/models.yml")?;
    assert_eq!(models.mode, FileMode(0o600));
    assert_eq!(
        models.text,
        "providers:\n  zai:\n    baseUrl: https://api.z.ai/api/anthropic\n    api: anthropic-messages\n    apiKey: ZAI_API_KEY\n"
    );

    let t14 = plan_static_files(
        &instance(Agent::ClaudeCode, Provider::Zai, "glm-5.3")?,
        &HOME,
        &WORKDIR,
        None,
    )?;
    assert_eq!(t14.len(), 2);
    let settings = file(&t14, "/sandbox/.claude/settings.json")?;
    assert_eq!(settings.mode, FileMode(0o600));
    assert_eq!(
        settings.text,
        "{\"env\":{\"ANTHROPIC_BASE_URL\":\"https://api.z.ai/api/anthropic\"},\"model\":\"glm-5.3\"}"
    );
    let credentials = file(&t14, "/sandbox/.claude/.credentials.json")?;
    assert_eq!(credentials.mode, FileMode(0o600));
    assert_eq!(credentials.text, "{}");

    let t18 = plan_static_files(
        &instance(Agent::Codex, Provider::Zai, "glm-5.3")?,
        &HOME,
        &WORKDIR,
        None,
    )?;
    let config = file(&t18, "/sandbox/.codex/config.toml")?;
    assert_eq!(config.mode, FileMode(0o600));
    assert_eq!(
        config.text,
        "model = \"glm-5.3\"\nmodel_provider = \"zai\"\n\n[model_providers.zai]\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"ZAI_API_KEY\"\nname = \"Z.AI\"\nwire_api = \"responses\"\n\n[projects.\"/sandbox/project\"]\ntrust_level = \"trusted\"\n"
    );
    Ok(())
}

#[test]
fn secondary_static_files_use_the_secondary_home_and_workdir_only() -> TestResult {
    let home = SandboxPath::parse("/sandbox/.jackin/homes/codex")?;
    let workdir = SandboxPath::parse("/sandbox/work/codex/project")?;
    let files = plan_static_files(
        &instance(Agent::Codex, Provider::Zai, "glm-5.3")?,
        &home,
        &workdir,
        None,
    )?;
    let config = file(&files, "/sandbox/.jackin/homes/codex/.codex/config.toml")?;
    assert!(
        config
            .text
            .contains("[projects.\"/sandbox/work/codex/project\"]")
    );
    assert!(!config.text.contains("[projects.\"/sandbox/project\"]"));
    assert!(
        !files
            .iter()
            .any(|file| file.path.as_str() == "/sandbox/.codex/config.toml")
    );
    assert!(
        !files
            .iter()
            .any(|file| file.path.as_str().contains("homes/claude"))
    );
    Ok(())
}

#[test]
fn claude_static_credentials_are_empty_for_every_static_tuple() -> TestResult {
    for provider in [
        Provider::Kimi,
        Provider::Zai,
        Provider::Minimax,
        Provider::Mimo,
    ] {
        let tuple = lookup_tuple(Agent::ClaudeCode, provider)?;
        let files = plan_static_files(
            &instance(tuple.agent, tuple.provider, tuple.default_model)?,
            &HOME,
            &WORKDIR,
            None,
        )?;
        let credentials = file(&files, "/sandbox/.claude/.credentials.json")?;
        assert_eq!(credentials.text, "{}", "{}", tuple.id);
        let settings = file(&files, "/sandbox/.claude/settings.json")?;
        for forbidden in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"] {
            assert!(!credentials.text.contains(forbidden), "{}", tuple.id);
            assert!(!settings.text.contains(forbidden), "{}", tuple.id);
        }
    }
    Ok(())
}

#[test]
fn custom_codex_static_files_never_acquire_t01_auth() -> TestResult {
    for provider in [
        Provider::Kimi,
        Provider::Zai,
        Provider::Minimax,
        Provider::Mimo,
    ] {
        let tuple = lookup_tuple(Agent::Codex, provider)?;
        let files = plan_static_files(
            &instance(tuple.agent, tuple.provider, tuple.default_model)?,
            &HOME,
            &WORKDIR,
            None,
        )?;
        assert!(
            !files
                .iter()
                .any(|file| file.path.as_str() == "/sandbox/.codex/auth.json"),
            "{}",
            tuple.id
        );
    }
    Ok(())
}

#[test]
fn native_subscription_tuples_do_not_gain_a_static_file_fallback() -> TestResult {
    for (agent, provider) in [
        (Agent::MinimaxCode, Provider::Minimax),
        (Agent::MimoCode, Provider::Mimo),
        (Agent::KimiCode, Provider::Kimi),
    ] {
        let tuple = lookup_tuple(agent, provider)?;
        let files = plan_static_files(
            &instance(agent, provider, tuple.default_model)?,
            &HOME,
            &WORKDIR,
            None,
        )?;
        assert!(files.is_empty(), "{}", tuple.id);
    }
    Ok(())
}

#[tokio::test]
async fn missing_models_use_tuple_defaults_and_explicit_models_remain_typed() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.manifest_text(ROLE.to_owned());
        let mut spec = Spec::new(role);
        spec.accounts = "[accounts.zai]\nprovider = \"zai\"\ncredential = { source = \"file\", path = \"/tmp/zai-key\" }\n".to_owned();
        let setup = spec.build().await?;
        for (id, agent) in [(1, Agent::Omp), (2, Agent::ClaudeCode), (3, Agent::Codex)] {
            let plan = setup.plan(setup.request(id)?.tuple(agent, Provider::Zai))?;
            assert_eq!(plan.instances().lead().agent, agent);
            assert_eq!(plan.instances().lead().provider, Provider::Zai);
            assert_eq!(plan.instances().lead().model, "glm-5.3");
            let explicit = setup.plan(
                setup
                    .request(id + 3)?
                    .tuple(agent, Provider::Zai)
                    .model(Some("glm-5.3".to_owned())),
            )?;
            assert_eq!(explicit.instances().lead().model, "glm-5.3");
            let error = setup
                .plan(
                    setup
                        .request(id + 6)?
                        .tuple(agent, Provider::Zai)
                        .model(Some("unknown".to_owned())),
                )
                .err()
                .ok_or("expected unknown-model refusal")?;
            assert!(matches!(error, PlanError::TupleNotSupported { ref reason, .. } if reason == "unknown model unknown"));
        }
        let error = setup
            .plan(setup.request(10)?.tuple(Agent::KimiCode, Provider::Kimi))
            .err()
            .ok_or("expected unavailable tuple refusal")?;
        assert!(matches!(error, PlanError::TupleNotSupported { ref reason, .. } if reason == "native refresh and final live proof pending"));
        Ok(())
    })
    .await
}
