// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, CatalogError, Provider, TupleState,
    lookup_tuple,
};
use jackin_session::{HeadState, PlanError, plan_multi};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard, login_account};

struct Case {
    id: &'static str,
    agent: Agent,
    provider: Provider,
    model: &'static str,
    reason: &'static str,
}

const T01: Case = Case {
    id: "T01",
    agent: Agent::Codex,
    provider: Provider::Codex,
    model: "gpt-6.1-sol",
    reason: "final live proof pending after host re-login",
};
const T10: Case = Case {
    id: "T10",
    agent: Agent::Omp,
    provider: Provider::Kimi,
    model: "kimi-for-coding",
    reason: "final live proof pending with Kimi plan key",
};
const T11: Case = Case {
    id: "T11",
    agent: Agent::Omp,
    provider: Provider::Minimax,
    model: "minimax-m3",
    reason: "final live proof pending with MiniMax plan key",
};
const T12: Case = Case {
    id: "T12",
    agent: Agent::Omp,
    provider: Provider::Mimo,
    model: "mimo-v2.6-pro",
    reason: "final live proof pending with MiMo Token Plan key",
};
const T16: Case = Case {
    id: "T16",
    agent: Agent::ClaudeCode,
    provider: Provider::Mimo,
    model: "mimo-v2.6-pro",
    reason: "final live proof pending with MiMo Token Plan key",
};
const T20: Case = Case {
    id: "T20",
    agent: Agent::Codex,
    provider: Provider::Mimo,
    model: "mimo-v2.6-pro",
    reason: "final live proof pending with MiMo Token Plan key",
};

fn role() -> Result<RoleFixture, Box<dyn std::error::Error>> {
    Ok(RoleFixture::new("sentinel")?.manifest_text(
        "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n\n[agents.grok]\ninstructions = \"instructions/grok.md\"\n".to_owned(),
    ))
}

async fn setup() -> Result<super::support::Setup, Box<dyn std::error::Error>> {
    let mut spec = Spec::new(role()?);
    spec.accounts = format!(
        "{}[accounts.zai]\nprovider = \"zai\"\ncredential = {{ source = \"env\", var = \"ZAI_API_KEY\" }}\n\n[accounts.grok]\nprovider = \"grok\"\ncredential = {{ source = \"host-login\", path = \"/tmp/grok-home/auth.json\" }}\n",
        login_account("work")
    );
    spec.build().await
}

fn instance(
    agent: Agent,
    provider: Provider,
    account: &str,
    model: &str,
) -> Result<AgentInstance, Box<dyn std::error::Error>> {
    Ok(AgentInstance::new(
        agent,
        provider,
        AccountName::parse(account)?,
        model.to_owned(),
    ))
}

fn with_secondary(case: &Case) -> Result<AgentInstances, Box<dyn std::error::Error>> {
    let (lead_agent, lead_model) = if case.agent == Agent::Codex {
        (Agent::ClaudeCode, "glm-5.3")
    } else {
        (Agent::Codex, "glm-5.3")
    };
    Ok(AgentInstances::new(vec![
        instance(lead_agent, Provider::Zai, "zai", lead_model)?,
        instance(case.agent, case.provider, "work", case.model)?,
    ])?)
}

fn assert_refusal(error: PlanError, case: &Case) {
    assert!(
        matches!(
            error,
            PlanError::TupleNotSupported {
                agent,
                provider,
                ref reason,
            } if agent == case.agent && provider == case.provider && reason == case.reason
        ),
        "{}: {error}",
        case.id
    );
}

async fn lead_refusal(case: &Case) -> TestResult {
    guard(async {
        let tuple = lookup_tuple(case.agent, case.provider)?;
        assert_eq!(tuple.id, case.id);
        assert_eq!(tuple.state, TupleState::Unavailable);
        assert_eq!(tuple.reason, Some(case.reason));
        assert_eq!(tuple.check_model(case.model)?.id, case.id);
        assert!(matches!(
            tuple.check_model("no-such-model"),
            Err(CatalogError::UnknownModel { .. })
        ));
        let setup = setup().await?;
        let error = setup
            .plan(
                setup
                    .request(1)?
                    .tuple(case.agent, case.provider)
                    .model(Some(case.model.to_owned())),
            )
            .err()
            .ok_or("expected lead refusal")?;
        assert_refusal(error, case);
        Ok(())
    })
    .await
}

async fn secondary_launch_refusal(case: &Case) -> TestResult {
    guard(async {
        let setup = setup().await?;
        let error = setup
            .plan(setup.request(1)?.instances(with_secondary(case)?))
            .err()
            .ok_or("expected secondary plan_launch refusal")?;
        assert_refusal(error, case);
        Ok(())
    })
    .await
}

fn secondary_multi_refusal(case: &Case) -> TestResult {
    let error = plan_multi(&with_secondary(case)?, None, HeadState::Born)
        .err()
        .ok_or("expected secondary plan_multi refusal")?;
    assert_refusal(error, case);
    Ok(())
}

macro_rules! refusal_tests {
    ($lead:ident, $launch:ident, $multi:ident, $case:ident) => {
        #[tokio::test]
        async fn $lead() -> TestResult {
            lead_refusal(&$case).await
        }

        #[tokio::test]
        async fn $launch() -> TestResult {
            secondary_launch_refusal(&$case).await
        }

        #[test]
        fn $multi() -> TestResult {
            secondary_multi_refusal(&$case)
        }
    };
}

refusal_tests!(
    t01_lead_refusal,
    t01_secondary_launch_refusal,
    t01_secondary_multi_refusal,
    T01
);
refusal_tests!(
    t10_lead_refusal,
    t10_secondary_launch_refusal,
    t10_secondary_multi_refusal,
    T10
);
refusal_tests!(
    t11_lead_refusal,
    t11_secondary_launch_refusal,
    t11_secondary_multi_refusal,
    T11
);
refusal_tests!(
    t12_lead_refusal,
    t12_secondary_launch_refusal,
    t12_secondary_multi_refusal,
    T12
);
refusal_tests!(
    t16_lead_refusal,
    t16_secondary_launch_refusal,
    t16_secondary_multi_refusal,
    T16
);
refusal_tests!(
    t20_lead_refusal,
    t20_secondary_launch_refusal,
    t20_secondary_multi_refusal,
    T20
);

#[test]
fn supported_controls_accept_their_declared_models() -> TestResult {
    for (agent, provider, model) in [
        (Agent::Omp, Provider::Zai, "glm-5.3"),
        (Agent::ClaudeCode, Provider::Zai, "glm-5.3"),
        (Agent::Codex, Provider::Zai, "glm-5.3"),
    ] {
        let tuple = lookup_tuple(agent, provider)?;
        assert_eq!(tuple.state, TupleState::Supported);
        assert_eq!(tuple.reason, None);
        assert_eq!(tuple.check_model(model)?.id, tuple.id);
        let instances = AgentInstances::single(instance(agent, provider, "zai", model)?);
        plan_multi(&instances, None, HeadState::Born)?;
    }
    Ok(())
}

#[tokio::test]
async fn t18_plan_launch_is_the_positive_control() -> TestResult {
    guard(async {
        let setup = setup().await?;
        let plan = setup.plan(
            setup
                .request(1)?
                .tuple(Agent::Codex, Provider::Zai)
                .account(Some(AccountName::parse("zai")?))
                .model(Some("glm-5.3".to_owned())),
        )?;
        assert_eq!(plan.instances().lead().model, "glm-5.3");
        Ok(())
    })
    .await
}

fn t03_secondary_instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        instance(Agent::Codex, Provider::Zai, "zai", "glm-5.3")?,
        instance(Agent::GrokBuild, Provider::Grok, "grok", "grok-4.7")?,
    ])?)
}

fn assert_grok_config(file: &jackin_session::PlannedFile) -> TestResult {
    let config: toml::Value = toml::from_str(&file.text)?;
    assert_eq!(config["models"]["default"].as_str(), Some("grok-4.7"));
    assert_eq!(config["auth"]["disable_api_key_auth"].as_bool(), Some(true));
    let helper = config["auth"]["auth_provider_command"]
        .as_str()
        .ok_or("missing Grok session helper")?;
    assert!(helper.contains("GROK_SESSION_TOKEN"));
    assert!(!file.text.contains("XAI_API_KEY"));
    assert!(!file.text.contains("refresh_token"));
    Ok(())
}

#[tokio::test]
async fn t03_plan_launch_accepts_default_and_explicit_models_with_trusted_session_auth()
-> TestResult {
    guard(async {
        let setup = setup().await?;
        for model in [None, Some("grok-4.7".to_owned())] {
            let plan = setup.plan(
                setup
                    .request(44)?
                    .tuple(Agent::GrokBuild, Provider::Grok)
                    .account(Some(AccountName::parse("grok")?))
                    .model(model),
            )?;
            let lead = plan.instances().lead();
            assert_eq!(lead.agent, Agent::GrokBuild);
            assert_eq!(lead.provider, Provider::Grok);
            assert_eq!(lead.account.as_str(), "grok");
            assert_eq!(lead.model, "grok-4.7");
            assert_eq!(plan.files().layout.exec[0].argv, ["grok", "--trust"]);
            assert_eq!(plan.files().static_files.len(), 1);
            let config = &plan.files().static_files[0];
            assert_eq!(config.path.as_str(), "/sandbox/.grok/config.toml");
            assert_grok_config(config)?;
            assert!(!config.path.as_str().ends_with("auth.json"));
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t03_is_accepted_as_a_secondary_launch_and_multi_instance() -> TestResult {
    guard(async {
        let setup = setup().await?;
        let instances = t03_secondary_instances()?;
        let layout = plan_multi(&instances, None, HeadState::Born)?;
        assert_eq!(layout.exec[1].argv, ["grok", "--trust"]);
        assert_eq!(layout.exec[1].model, "grok-4.7");
        let plan = setup.plan(setup.request(45)?.instances(instances))?;
        let secondary = plan.instances().iter().nth(1).ok_or("missing T03")?;
        assert_eq!(secondary.agent, Agent::GrokBuild);
        assert_eq!(secondary.provider, Provider::Grok);
        assert_eq!(secondary.account.as_str(), "grok");
        assert_eq!(secondary.model, "grok-4.7");
        let config = plan
            .files()
            .static_files
            .iter()
            .find(|file| file.path.as_str() == "/sandbox/.jackin/homes/grok/.grok/config.toml")
            .ok_or("missing secondary Grok config")?;
        assert_grok_config(config)?;
        assert!(
            plan.files()
                .static_files
                .iter()
                .all(|file| { !file.path.as_str().ends_with("/.grok/auth.json") })
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t03_plan_launch_rejects_the_effective_reply_model() -> TestResult {
    guard(async {
        let setup = setup().await?;
        let error = setup
            .plan(
                setup
                    .request(46)?
                    .tuple(Agent::GrokBuild, Provider::Grok)
                    .account(Some(AccountName::parse("grok")?))
                    .model(Some("grok-4.7-build".to_owned())),
            )
            .err()
            .ok_or("effective reply model was accepted")?;
        assert!(matches!(
            error,
            PlanError::TupleNotSupported { ref reason, .. }
                if reason == "unknown model grok-4.7-build"
        ));
        Ok(())
    })
    .await
}
