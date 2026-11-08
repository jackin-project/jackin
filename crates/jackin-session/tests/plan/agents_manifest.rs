// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, ManifestError, ModelName, Provider,
};
use jackin_session::PlanError;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

fn codex_and(
    agent: Agent,
    provider: Provider,
    model: &str,
) -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Codex,
            Provider::Codex,
            AccountName::parse("work")?,
            ModelName::parse("gpt-5.1-codex")?,
        ),
        AgentInstance::new(
            agent,
            provider,
            AccountName::parse("work")?,
            ModelName::parse(model)?,
        ),
    ])?)
}

#[tokio::test]
async fn agent_instruction_mapping_is_checked_after_tuple_support() -> TestResult {
    guard(async {
        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let err = setup
            .plan(setup.request(1)?.instances(codex_and(Agent::Omp, Provider::Zai, "glm-5.3")?))
            .err()
            .ok_or("expected error")?;
        assert!(
            matches!(
                err,
                PlanError::Manifest(ManifestError::NoInstructionMapping {
                    ref role,
                    agent: Agent::Omp
                }) if role.as_str() == "sentinel"
            ),
            "{err}"
        );
        let plan = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.account().account.as_str(), "work");

        let role = RoleFixture::new("sentinel")?.manifest_text(
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n".to_owned(),
        );
        let setup = Spec::new(role).build().await?;
        let err = setup
            .plan(setup.request(3)?.instances(codex_and(Agent::Omp, Provider::Kimi, "kimi-for-coding")?))
            .err()
            .ok_or("expected error")?;
        assert!(matches!(err, PlanError::TupleNotSupported { agent: Agent::Omp, provider: Provider::Kimi, .. }), "{err}");
        Ok(())
    })
    .await
}
